use std::sync::Arc;

use providerdeck_core::routes::{BridgeContext, BridgeRuntimeService, handle_bridge_request};
use serde_json::{Value, json};

struct FakeRuntime;

#[async_trait::async_trait]
impl BridgeRuntimeService for FakeRuntime {
    async fn open_devtools(&self) -> anyhow::Result<Value> {
        Ok(json!({"status":"ok"}))
    }
    async fn open_manager(&self) -> anyhow::Result<Value> {
        Ok(json!({"status":"ok"}))
    }
    async fn backend_status(&self) -> anyhow::Result<Value> {
        Ok(json!({"status":"ok"}))
    }
    async fn repair_backend(&self) -> anyhow::Result<Value> {
        Ok(json!({"status":"ok"}))
    }
}

fn context() -> BridgeContext {
    BridgeContext::core(Arc::new(FakeRuntime))
}

#[tokio::test]
async fn backend_status_is_dispatched_to_runtime() {
    let response = handle_bridge_request(context(), "/backend/status", json!({})).await;
    assert_eq!(response["status"], "ok");
}

#[tokio::test]
async fn legacy_history_routes_are_not_exposed() {
    for path in ["/delete", "/undo", "/export-markdown", "/thread-sort-key"] {
        let response =
            handle_bridge_request(context(), path, json!({"session_id":"thread-1"})).await;
        assert_eq!(response["status"], "failed");
        assert_eq!(response["message"], "Unknown bridge path");
    }
}

#[tokio::test]
async fn journal_rejects_renderer_credentials() {
    let response = handle_bridge_request(
        context(),
        "/providerdeck/switch-journal/save",
        json!({"phase":"switching", "apiKey":"secret"}),
    )
    .await;
    assert_eq!(response["status"], "failed");
    assert!(
        response["message"]
            .as_str()
            .unwrap()
            .contains("credentials")
    );
}
