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
const RENDERER_INCOMING_HOOK: &str = "__providerDeckInterceptIncomingMessage";
const RENDERER_BRIDGE_PATCH_LOADED: &str = "__providerDeckTransportPatchLoaded";
const APP_SERVER_HANDLER_ERROR: &str = "Missing AppServer request message handler";
const SEND_CLI_REQUEST_COMMAND: &str = "send-cli-request-for-host";
const CDP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CDP_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

pub type BridgeHandler = Arc<
    dyn Fn(String, Value) -> Pin<Box<dyn Future<Output = anyhow::Result<Value>> + Send>>
        + Send
        + Sync,
>;

static NEXT_MESSAGE_ID: AtomicU64 = AtomicU64::new(100);

pub fn patch_renderer_bridge_source(source: &str) -> anyhow::Result<Option<String>> {
    let mut patch_transport =
        !source.contains(RENDERER_BRIDGE_HOOK) && source.contains(RENDERER_BRIDGE_EVENT);
    let patch_gateway = !source.contains("__providerDeckSendCliRequest")
        && source.contains(APP_SERVER_HANDLER_ERROR);
    let bridge = Regex::new(
        r"postMessage:([A-Za-z_$][A-Za-z0-9_$]*)=>\{let ([A-Za-z_$][A-Za-z0-9_$]*)=!1,([A-Za-z_$][A-Za-z0-9_$]*)=window\.electronBridge;",
    )?;
    let incoming = Regex::new(
        r"handleMessage\(([A-Za-z_$][A-Za-z0-9_$]*)\)\{let ([A-Za-z_$][A-Za-z0-9_$]*)=([A-Za-z_$][A-Za-z0-9_$]*)\(([A-Za-z_$][A-Za-z0-9_$]*)\);([A-Za-z_$][A-Za-z0-9_$]*)!=null&&this\.deliverMessage\(([A-Za-z_$][A-Za-z0-9_$]*)\.type,([A-Za-z_$][A-Za-z0-9_$]*)\)",
    )?;
    if patch_transport {
        let matches = bridge.find_iter(source).count();
        if matches > 1 {
            bail!("multiple Codex renderer bridges matched; refusing ambiguous patch");
        }
        patch_transport = matches == 1;
    }
    if !patch_transport && !patch_gateway {
        return Ok(None);
    }

    let mut patched = source.to_string();
    if patch_gateway {
        let marker = source
            .find(APP_SERVER_HANDLER_ERROR)
            .context("Codex AppServer request client marker is missing")?;
        let mut gateway_end = source.len().min(marker + 2048);
        while !source.is_char_boundary(gateway_end) {
            gateway_end -= 1;
        }
        let gateway_source = &source[marker..gateway_end];
        let gateway = Regex::new(
            r"function\s+([A-Za-z_$][A-Za-z0-9_$]*)\(([A-Za-z_$][A-Za-z0-9_$]*),([A-Za-z_$][A-Za-z0-9_$]*)\)\{return\s+([A-Za-z_$][A-Za-z0-9_$]*)\.sendRequest\(([A-Za-z_$][A-Za-z0-9_$]*),([A-Za-z_$][A-Za-z0-9_$]*)\)\}",
        )?;
        let gateways = gateway
            .captures_iter(gateway_source)
            .filter(|captures| captures[2] == captures[5] && captures[3] == captures[6])
            .collect::<Vec<_>>();
        if gateways.len() != 1 {
            bail!(
                "expected one Codex AppServer request gateway near its handler, found {}",
                gateways.len()
            );
        }
        let gateway_match = gateways[0]
            .get(0)
            .context("Codex AppServer request gateway match is missing")?;
        let gateway_name = &gateways[0][1];
        let insertion_offset = marker + gateway_match.end();
        patched = format!(
            "{};window.__providerDeckSendCliRequest=payload=>[`thread/read`,`thread/unsubscribe`,`thread/resume`,`thread/compact/start`].includes(payload?.method)?{gateway_name}(`{SEND_CLI_REQUEST_COMMAND}`,payload):Promise.reject(Error(`Unsupported ProviderDeck AppServer request`));{}",
            &source[..insertion_offset],
            &source[insertion_offset..]
        );
    }

    if !patch_transport {
        return Ok(Some(patched));
    }
    let replacement = format!(
        "postMessage:${{1}}=>{{if(window.{RENDERER_BRIDGE_HOOK}?.(${{1}})===true)return;let ${{2}}=!1,${{3}}=window.electronBridge;"
    );
    let patched = bridge.replace(&patched, replacement);
    let incoming_matches = incoming
        .captures_iter(&patched)
        .filter(|captures| {
            captures[1] == captures[4]
                && captures[2] == captures[5]
                && captures[2] == captures[6]
                && captures[2] == captures[7]
        })
        .collect::<Vec<_>>();
    if incoming_matches.len() != 1 {
        bail!(
            "expected one Codex renderer incoming message dispatcher, found {}",
            incoming_matches.len()
        );
    }
    let captures = &incoming_matches[0];
    let incoming_match = captures
        .get(0)
        .context("Codex renderer incoming dispatcher match is missing")?;
    let incoming_range = incoming_match.start()..incoming_match.end();
    let event = captures[1].to_string();
    let message = captures[2].to_string();
    let parser = captures[3].to_string();
    let incoming_replacement = format!(
        "handleMessage({event}){{let {message}={parser}({event});{message}=window.{RENDERER_INCOMING_HOOK}?.({message})??{message};{message}!=null&&this.deliverMessage({message}.type,{message})"
    );
    drop(incoming_matches);
    let mut patched = patched.into_owned();
    patched.replace_range(incoming_range, &incoming_replacement);
    Ok(Some(format!(
        "window.{RENDERER_BRIDGE_PATCH_LOADED}=true;window.__providerDeckPendingPostMessages=window.__providerDeckPendingPostMessages||[];window.{RENDERER_INCOMING_HOOK}=window.{RENDERER_INCOMING_HOOK}||function(message){{return message}};window.{RENDERER_BRIDGE_HOOK}=window.{RENDERER_BRIDGE_HOOK}||function(detail){{if(![`model/list`,`config/value/write`,`config/batchWrite`,`thread/start`,`turn/start`].includes(detail?.request?.method))return false;window.__providerDeckPendingPostMessages.push(detail);return true}};{patched}"
    )))
}

pub fn renderer_bridge_fetch_enable_params() -> Value {
    json!({
        "patterns": [{
            "urlPattern": "*app-initial~*.js*",
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
    // 禁用 HTTP 缓存，确保 renderer bundle 必重新请求被 Fetch 拦截 patch
    // （否则 bundle 缓存命中时不重新请求，transport patch 无法注入）。
    // 用非阻塞发送：Network.enable 会触发大量已完成请求的 catch-up 事件，
    // 阻塞等待响应会 5s 超时导致整个 prearm 失败。CDP 按顺序处理命令，
    // setCacheDisabled 仍在 runIfWaitingForDebugger 前生效。
    session
        .send_command_without_wait(next_message_id(), "Network.enable", json!({}))
        .await?;
    session
        .send_command_without_wait(
            next_message_id(),
            "Network.setCacheDisabled",
            json!({ "cacheDisabled": true }),
        )
        .await?;
    let _ = crate::diagnostic_log::append_diagnostic_log(
        "renderer_transport.cache_disable_sent",
        json!({ "session_id": page_session_id }),
    );
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
  if (window.__providerDeckBridge) return;
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

/// 强制页面重载并绕过缓存。当 renderer bundle 在 Fetch 拦截器就绪前已加载完毕、
/// transport patch 未注入时，用此方法触发 bundle 重新请求，让已就绪的 Fetch
/// 拦截器拦截并注入 patch。ignoreCache=true 等效 Shift+Ctrl+R 硬重载，
/// 绕过所有子资源（含 bundle）的 HTTP 缓存。
pub async fn reload_page_ignore_cache(websocket_url: &str) -> anyhow::Result<()> {
    let socket = connect_cdp_websocket(websocket_url).await?;
    let mut session = CdpSession::new(socket);
    session.send_command(1, "Page.enable", json!({})).await?;
    session
        .send_command(2, "Page.reload", json!({ "ignoreCache": true }))
        .await?;
    Ok(())
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
        let mut command = json!({
            "id": message_id,
            "method": method,
            "params": params,
        });
        if let Some(session_id) = self.session_id.clone() {
            command["sessionId"] = Value::String(session_id);
        }
        self.socket
            .send(Message::Text(command.to_string().into()))
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
