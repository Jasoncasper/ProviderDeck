use std::collections::HashMap;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, bail};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use regex::Regex;
use serde_json::{Value, json};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

pub const BRIDGE_BINDING_NAME: &str = "providerDeckBridgeV1";
const RENDERER_BRIDGE_EVENT: &str = "codex-message-from-view";
const RENDERER_BRIDGE_HOOK: &str = "__providerDeckInterceptPostMessage";
const RENDERER_BRIDGE_PATCH_LOADED: &str = "__providerDeckTransportPatchLoaded";
const CDP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CDP_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

pub type BridgeHandler = Arc<
    dyn Fn(String, Value) -> Pin<Box<dyn Future<Output = anyhow::Result<Value>> + Send>>
        + Send
        + Sync,
>;

static NEXT_MESSAGE_ID: AtomicU64 = AtomicU64::new(100);

pub fn patch_renderer_bridge_source(source: &str) -> anyhow::Result<Option<String>> {
    if source.contains(RENDERER_BRIDGE_HOOK) || !source.contains(RENDERER_BRIDGE_EVENT) {
        return Ok(None);
    }
    let bridge = Regex::new(
        r"postMessage:([A-Za-z_$][A-Za-z0-9_$]*)=>\{let ([A-Za-z_$][A-Za-z0-9_$]*)=!1,([A-Za-z_$][A-Za-z0-9_$]*)=window\.electronBridge;",
    )?;
    let matches = bridge.find_iter(source).count();
    if matches == 0 {
        return Ok(None);
    }
    if matches > 1 {
        bail!("multiple Codex renderer bridges matched; refusing ambiguous patch");
    }
    let replacement = format!(
        "postMessage:${{1}}=>{{if(window.{RENDERER_BRIDGE_HOOK}?.(${{1}})===true)return;let ${{2}}=!1,${{3}}=window.electronBridge;"
    );
    let patched = bridge.replace(source, replacement);
    Ok(Some(format!(
        "window.{RENDERER_BRIDGE_PATCH_LOADED}=true;window.__providerDeckPendingPostMessages=window.__providerDeckPendingPostMessages||[];window.{RENDERER_BRIDGE_HOOK}=window.{RENDERER_BRIDGE_HOOK}||function(detail){{window.__providerDeckPendingPostMessages.push(detail);return true}};{patched}"
    )))
}

pub fn renderer_bridge_fetch_enable_params() -> Value {
    json!({
        "patterns": [{
            "urlPattern": "*app-initial~app-main~*.js*",
            "resourceType": "Script",
            "requestStage": "Response"
        }]
    })
}

pub fn renderer_prearm_auto_attach_params() -> Value {
    json!({
        "autoAttach": true,
        "waitForDebuggerOnStart": true,
        "flatten": true,
        "filter": [
            { "type": "page", "exclude": false },
            { "exclude": true }
        ]
    })
}

pub async fn prearm_renderer_bridge_interceptor(websocket_url: &str) -> anyhow::Result<()> {
    let socket = connect_cdp_websocket(websocket_url).await?;
    let mut session = CdpSession::new(socket);
    session
        .send_command(
            1,
            "Target.setAutoAttach",
            renderer_prearm_auto_attach_params(),
        )
        .await?;

    let page_session_id = loop {
        if let Some(attached) = session.target_attached_calls.pop_front() {
            if let Some(session_id) = attached_page_session_id(&attached) {
                break session_id;
            }
            resume_attached_target(&mut session, &attached).await?;
            continue;
        }
        let Some(_) = session.next_message().await? else {
            bail!("CDP browser websocket closed before a page target was attached");
        };
    };

    session.session_id = Some(page_session_id.clone());
    session
        .send_command(
            next_message_id(),
            "Fetch.enable",
            renderer_bridge_fetch_enable_params(),
        )
        .await?;
    session
        .send_command(
            next_message_id(),
            "Runtime.runIfWaitingForDebugger",
            json!({}),
        )
        .await?;

    let _ = crate::diagnostic_log::append_diagnostic_log(
        "renderer_transport.prearmed",
        json!({ "session_id": page_session_id }),
    );
    tokio::spawn(async move {
        loop {
            if session.drain_target_attached_queue().await.is_err() {
                break;
            }
            if session.drain_fetch_queue().await.is_err() {
                break;
            }
            match session.next_message().await {
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }
    });
    Ok(())
}

fn attached_page_session_id(message: &Value) -> Option<String> {
    let params = message.get("params")?;
    (params.pointer("/targetInfo/type").and_then(Value::as_str) == Some("page"))
        .then(|| {
            params
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .flatten()
}

async fn resume_attached_target<S>(
    session: &mut CdpSession<S>,
    message: &Value,
) -> anyhow::Result<()>
where
    S: SinkExt<Message>
        + StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin
        + Send,
    <S as futures_util::Sink<Message>>::Error: std::error::Error + Send + Sync + 'static,
{
    let Some(session_id) = message.pointer("/params/sessionId").and_then(Value::as_str) else {
        return Ok(());
    };
    session
        .send_command_for_session(
            next_message_id(),
            "Runtime.runIfWaitingForDebugger",
            json!({}),
            Some(session_id),
        )
        .await?;
    Ok(())
}

pub fn build_bridge_script(binding_name: &str) -> String {
    format!(
        r#"
(() => {{
  window.__providerDeckCallbacks = new Map();
  window.__providerDeckSeq = 0;
  window.__providerDeckResolve = (id, result) => {{
    const callback = window.__providerDeckCallbacks.get(id);
    if (!callback) return;
    window.__providerDeckCallbacks.delete(id);
    callback.resolve(result);
  }};
  window.__providerDeckReject = (id, message) => {{
    const callback = window.__providerDeckCallbacks.get(id);
    if (!callback) return;
    window.__providerDeckCallbacks.delete(id);
    callback.resolve({{ status: "failed", message }});
  }};
  window.__providerDeckBridge = (path, payload) => new Promise((resolve) => {{
    const id = String(++window.__providerDeckSeq);
    window.__providerDeckCallbacks.set(id, {{ resolve }});
    window.{binding_name}(JSON.stringify({{ id, path, payload }}));
  }});
}})();
"#
    )
}

pub fn bridge_health_check_script() -> &'static str {
    r#"
(() => {
  const bridge = window.__providerDeckBridge;
  if (typeof bridge !== "function") return false;
  try {
    return Promise.race([
      Promise.resolve(bridge("/backend/status", {})).then((result) => !!result && result.status === "ok"),
      new Promise((resolve) => setTimeout(() => resolve(false), 2000)),
    ]);
  } catch (error) {
    return false;
  }
})()
"#
}

pub async fn evaluate_script(websocket_url: &str, script: &str) -> anyhow::Result<Value> {
    evaluate_script_with_await_promise(websocket_url, script, false).await
}

pub async fn evaluate_script_with_await_promise(
    websocket_url: &str,
    script: &str,
    await_promise: bool,
) -> anyhow::Result<Value> {
    let socket = connect_cdp_websocket(websocket_url).await?;
    let mut session = CdpSession::new(socket);
    session
        .send_command(
            1,
            "Runtime.evaluate",
            runtime_evaluate_params_with_await_promise(script, await_promise),
        )
        .await
}

pub async fn add_script_to_new_documents(
    websocket_url: &str,
    script: &str,
) -> anyhow::Result<Value> {
    let socket = connect_cdp_websocket(websocket_url).await?;
    let mut session = CdpSession::new(socket);
    session
        .send_command(
            1,
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": script }),
        )
        .await
}

pub async fn install_bridge(
    websocket_url: &str,
    binding_name: &str,
    handler: BridgeHandler,
    new_document_scripts: &[String],
) -> anyhow::Result<()> {
    let socket = connect_cdp_websocket(websocket_url).await?;
    let mut session = CdpSession::new(socket).with_handler(handler);

    session.send_command(1, "Runtime.enable", json!({})).await?;
    session
        .send_command(2, "Runtime.removeBinding", json!({ "name": binding_name }))
        .await?;
    session
        .send_command(3, "Runtime.addBinding", json!({ "name": binding_name }))
        .await?;

    let bridge_script = build_bridge_script(binding_name);
    session
        .send_command(
            4,
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": bridge_script }),
        )
        .await?;
    session
        .send_command(
            5,
            "Runtime.evaluate",
            runtime_evaluate_params(&bridge_script),
        )
        .await?;

    for script in new_document_scripts {
        let message_id = next_message_id();
        session
            .send_command(
                message_id,
                "Page.addScriptToEvaluateOnNewDocument",
                json!({ "source": script }),
            )
            .await?;
        let message_id = next_message_id();
        session
            .send_command(
                message_id,
                "Runtime.evaluate",
                runtime_evaluate_params(script),
            )
            .await?;
    }

    session.drain_binding_queue().await?;
    tokio::spawn(async move {
        loop {
            if session.drain_binding_queue().await.is_err() {
                break;
            }
            match session.next_message().await {
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }
    });

    Ok(())
}

pub fn runtime_evaluate_params(script: &str) -> Value {
    runtime_evaluate_params_with_await_promise(script, false)
}

pub fn runtime_evaluate_params_with_await_promise(script: &str, await_promise: bool) -> Value {
    json!({
        "expression": script,
        "awaitPromise": await_promise,
        "allowUnsafeEvalBlockedByCSP": true,
    })
}

pub fn runtime_evaluate_bool(response: &Value) -> Option<bool> {
    response
        .pointer("/result/result/value")
        .and_then(Value::as_bool)
}

pub fn resolve_bridge_expression(request_id: &str, result: &Value) -> anyhow::Result<String> {
    Ok(format!(
        "window.__providerDeckResolve({}, {})",
        serde_json::to_string(request_id)?,
        serde_json::to_string(result)?,
    ))
}

pub fn reject_bridge_expression(request_id: &str, message: &str) -> anyhow::Result<String> {
    Ok(format!(
        "window.__providerDeckReject({}, {})",
        serde_json::to_string(request_id)?,
        serde_json::to_string(message)?,
    ))
}

async fn connect_cdp_websocket(
    websocket_url: &str,
) -> anyhow::Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
    let (socket, _) = tokio::time::timeout(CDP_CONNECT_TIMEOUT, connect_async(websocket_url))
        .await
        .with_context(|| {
            format!(
                "timed out connecting CDP websocket after {}s",
                CDP_CONNECT_TIMEOUT.as_secs()
            )
        })?
        .context("failed to connect CDP websocket")?;

    Ok(socket)
}

struct CdpSession<S> {
    socket: S,
    responses: HashMap<u64, Value>,
    binding_calls: VecDeque<Value>,
    fetch_calls: VecDeque<Value>,
    target_attached_calls: VecDeque<Value>,
    handler: Option<BridgeHandler>,
    session_id: Option<String>,
}

impl<S> CdpSession<S>
where
    S: SinkExt<Message>
        + StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin
        + Send,
    <S as futures_util::Sink<Message>>::Error: std::error::Error + Send + Sync + 'static,
{
    fn new(socket: S) -> Self {
        Self {
            socket,
            responses: HashMap::new(),
            binding_calls: VecDeque::new(),
            fetch_calls: VecDeque::new(),
            target_attached_calls: VecDeque::new(),
            handler: None,
            session_id: None,
        }
    }

    fn with_handler(mut self, handler: BridgeHandler) -> Self {
        self.handler = Some(handler);
        self
    }

    async fn send_command(
        &mut self,
        message_id: u64,
        method: &str,
        params: Value,
    ) -> anyhow::Result<Value> {
        let session_id = self.session_id.clone();
        self.send_command_for_session(message_id, method, params, session_id.as_deref())
            .await
    }

    async fn send_command_for_session(
        &mut self,
        message_id: u64,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> anyhow::Result<Value> {
        let mut command = json!({
            "id": message_id,
            "method": method,
            "params": params,
        });
        if let Some(session_id) = session_id {
            command["sessionId"] = Value::String(session_id.to_string());
        }
        self.socket
            .send(Message::Text(command.to_string().into()))
            .await
            .with_context(|| format!("failed to send CDP command {method} id {message_id}"))?;

        tokio::time::timeout(
            CDP_COMMAND_TIMEOUT,
            self.wait_for_id(message_id, method.to_string()),
        )
        .await
        .with_context(|| {
            format!(
                "timed out waiting for CDP command {method} id {message_id} response after {}s",
                CDP_COMMAND_TIMEOUT.as_secs()
            )
        })?
    }

    async fn send_command_without_wait(
        &mut self,
        message_id: u64,
        method: &str,
        params: Value,
    ) -> anyhow::Result<()> {
        self.socket
            .send(Message::Text(
                json!({
                    "id": message_id,
                    "method": method,
                    "params": params,
                })
                .to_string()
                .into(),
            ))
            .await
            .with_context(|| format!("failed to send CDP command {method} id {message_id}"))?;
        Ok(())
    }

    async fn wait_for_id(&mut self, message_id: u64, method: String) -> anyhow::Result<Value> {
        loop {
            if let Some(response) = self.responses.remove(&message_id) {
                return command_result(response, &method, message_id);
            }

            let Some(message) = self.next_message().await? else {
                bail!("CDP websocket closed before response for {method} id {message_id}");
            };

            if let Some(response_id) = message.get("id").and_then(Value::as_u64) {
                if response_id == message_id {
                    return command_result(message, &method, message_id);
                }
                self.responses.insert(response_id, message);
            }
        }
    }

    async fn next_message(&mut self) -> anyhow::Result<Option<Value>> {
        let Some(message) = self.socket.next().await else {
            return Ok(None);
        };
        let message = message.context("failed to read CDP websocket message")?;
        let Message::Text(text) = message else {
            return Ok(Some(json!({})));
        };
        let value: Value = serde_json::from_str(&text).context("failed to parse CDP message")?;

        if value.get("method").and_then(Value::as_str) == Some("Runtime.bindingCalled") {
            self.binding_calls.push_back(value.clone());
        }
        if value.get("method").and_then(Value::as_str) == Some("Fetch.requestPaused") {
            self.fetch_calls.push_back(value.clone());
        }
        if value.get("method").and_then(Value::as_str) == Some("Target.attachedToTarget") {
            self.target_attached_calls.push_back(value.clone());
        }

        Ok(Some(value))
    }

    async fn drain_binding_queue(&mut self) -> anyhow::Result<()> {
        while let Some(message) = self.binding_calls.pop_front() {
            self.route_binding_call(message).await?;
        }
        Ok(())
    }

    async fn drain_fetch_queue(&mut self) -> anyhow::Result<()> {
        while let Some(message) = self.fetch_calls.pop_front() {
            self.route_fetch_call(message).await?;
        }
        Ok(())
    }

    async fn drain_target_attached_queue(&mut self) -> anyhow::Result<()> {
        while let Some(message) = self.target_attached_calls.pop_front() {
            let Some(session_id) = attached_page_session_id(&message) else {
                resume_attached_target(self, &message).await?;
                continue;
            };
            self.send_command_for_session(
                next_message_id(),
                "Fetch.enable",
                renderer_bridge_fetch_enable_params(),
                Some(&session_id),
            )
            .await?;
            self.send_command_for_session(
                next_message_id(),
                "Runtime.runIfWaitingForDebugger",
                json!({}),
                Some(&session_id),
            )
            .await?;
        }
        Ok(())
    }

    async fn route_fetch_call(&mut self, message: Value) -> anyhow::Result<()> {
        let event_session_id = message
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| self.session_id.clone());
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        let request_id = params
            .get("requestId")
            .and_then(Value::as_str)
            .context("Fetch.requestPaused missing requestId")?;
        let response_code = params
            .get("responseStatusCode")
            .and_then(Value::as_u64)
            .unwrap_or(200);
        let response_headers = params
            .get("responseHeaders")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let body = self
            .send_command_for_session(
                next_message_id(),
                "Fetch.getResponseBody",
                json!({ "requestId": request_id }),
                event_session_id.as_deref(),
            )
            .await?;
        let body_text = body
            .pointer("/result/body")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let bytes = if body
            .pointer("/result/base64Encoded")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            base64::engine::general_purpose::STANDARD
                .decode(body_text)
                .context("failed to decode intercepted renderer bundle")?
        } else {
            body_text.as_bytes().to_vec()
        };
        let source =
            String::from_utf8(bytes).context("intercepted renderer bundle is not UTF-8")?;
        let patch = patch_renderer_bridge_source(&source)?;
        if patch.is_some() {
            let _ = crate::diagnostic_log::append_diagnostic_log(
                "renderer_transport.patch_applied",
                json!({ "request_id": request_id }),
            );
        }
        let patched = patch.unwrap_or(source);
        let headers = response_headers
            .into_iter()
            .filter(|header| {
                !header
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| {
                        name.eq_ignore_ascii_case("content-length")
                            || name.eq_ignore_ascii_case("content-encoding")
                    })
            })
            .collect::<Vec<_>>();
        self.send_command_for_session(
            next_message_id(),
            "Fetch.fulfillRequest",
            json!({
                "requestId": request_id,
                "responseCode": response_code,
                "responseHeaders": headers,
                "body": base64::engine::general_purpose::STANDARD.encode(patched.as_bytes())
            }),
            event_session_id.as_deref(),
        )
        .await?;
        Ok(())
    }

    fn route_binding_call(
        &mut self,
        message: Value,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            let Some(handler) = self.handler.clone() else {
                return Ok(());
            };

            let Some(payload_text) = message
                .get("params")
                .and_then(|params| params.get("payload"))
                .and_then(Value::as_str)
            else {
                return Ok(());
            };

            let parsed: Value = match serde_json::from_str(payload_text) {
                Ok(parsed) => parsed,
                Err(error) => {
                    if let Some(request_id) = extract_string_field(payload_text, "id") {
                        self.reject_bridge_request(
                            &request_id,
                            &format!("failed to parse bridge payload: {error}"),
                        )
                        .await?;
                    }
                    return Ok(());
                }
            };
            self.route_parsed_binding_call(&handler, parsed).await
        })
    }

    async fn route_parsed_binding_call(
        &mut self,
        handler: &BridgeHandler,
        parsed: Value,
    ) -> anyhow::Result<()> {
        let Some(request_id) = parsed.get("id").and_then(Value::as_str) else {
            return Ok(());
        };
        let path = parsed
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let payload = parsed.get("payload").cloned().unwrap_or_else(|| json!({}));

        match handler(path, payload).await {
            Ok(result) => {
                self.resolve_bridge_request(request_id, &result).await?;
            }
            Err(error) => {
                self.reject_bridge_request(request_id, &error.to_string())
                    .await?;
            }
        }

        Ok(())
    }

    async fn resolve_bridge_request(
        &mut self,
        request_id: &str,
        result: &Value,
    ) -> anyhow::Result<()> {
        let expression = resolve_bridge_expression(request_id, result)?;
        let message_id = next_message_id();
        let _ = crate::diagnostic_log::append_diagnostic_log(
            "bridge.resolve_start",
            json!({
                "request_id": request_id,
                "message_id": message_id,
                "result_status": result.get("status").and_then(Value::as_str).unwrap_or("")
            }),
        );
        let sent = self
            .send_command_without_wait(
                message_id,
                "Runtime.evaluate",
                runtime_evaluate_params(&expression),
            )
            .await;
        match &sent {
            Ok(_) => {
                let _ = crate::diagnostic_log::append_diagnostic_log(
                    "bridge.resolve_ok",
                    json!({
                        "request_id": request_id,
                        "message_id": message_id
                    }),
                );
            }
            Err(error) => {
                let _ = crate::diagnostic_log::append_diagnostic_log(
                    "bridge.resolve_failed",
                    json!({
                        "request_id": request_id,
                        "message_id": message_id,
                        "message": error.to_string()
                    }),
                );
            }
        }
        sent.map(|_| ())
    }

    async fn reject_bridge_request(
        &mut self,
        request_id: &str,
        message: &str,
    ) -> anyhow::Result<()> {
        let expression = reject_bridge_expression(request_id, message)?;
        let message_id = next_message_id();
        let _ = crate::diagnostic_log::append_diagnostic_log(
            "bridge.reject_start",
            json!({
                "request_id": request_id,
                "message_id": message_id,
                "message": message
            }),
        );
        let sent = self
            .send_command_without_wait(
                message_id,
                "Runtime.evaluate",
                runtime_evaluate_params(&expression),
            )
            .await;
        match &sent {
            Ok(_) => {
                let _ = crate::diagnostic_log::append_diagnostic_log(
                    "bridge.reject_ok",
                    json!({
                        "request_id": request_id,
                        "message_id": message_id
                    }),
                );
            }
            Err(error) => {
                let _ = crate::diagnostic_log::append_diagnostic_log(
                    "bridge.reject_failed",
                    json!({
                        "request_id": request_id,
                        "message_id": message_id,
                        "error": error.to_string()
                    }),
                );
            }
        }
        sent.map(|_| ())
    }
}

fn command_result(response: Value, method: &str, message_id: u64) -> anyhow::Result<Value> {
    if let Some(error) = response.get("error") {
        bail!("CDP command {method} id {message_id} failed: {error}");
    }
    Ok(response)
}

fn extract_string_field(input: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let mut index = input.find(&needle)? + needle.len();
    let bytes = input.as_bytes();

    while matches!(bytes.get(index), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        index += 1;
    }
    if bytes.get(index) != Some(&b':') {
        return None;
    }
    index += 1;
    while matches!(bytes.get(index), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        index += 1;
    }
    if bytes.get(index) != Some(&b'"') {
        return None;
    }
    index += 1;

    let mut output = String::new();
    let mut escaped = false;
    for ch in input[index..].chars() {
        if escaped {
            output.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(output),
            _ => output.push(ch),
        }
    }

    None
}

fn next_message_id() -> u64 {
    NEXT_MESSAGE_ID.fetch_add(1, Ordering::Relaxed) + 1
}
