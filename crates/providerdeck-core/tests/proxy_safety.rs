use std::collections::HashMap;

use providerdeck_core::proxy::{network_safety_for_proxy, proxy_url_from_environment};

#[test]
fn proxy_url_prefers_https_environment_for_codex_requests() {
    let env = HashMap::from([
        (
            "HTTP_PROXY".to_string(),
            "http://plain-proxy.example.test:8080".to_string(),
        ),
        (
            "HTTPS_PROXY".to_string(),
            "http://secure-proxy.example.test:8443".to_string(),
        ),
    ]);

    assert_eq!(
        proxy_url_from_environment(&env).as_deref(),
        Some("http://secure-proxy.example.test:8443")
    );
}

#[tokio::test]
async fn network_safety_rejects_an_unreachable_loopback_proxy_without_exposing_credentials() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let result = network_safety_for_proxy(Some(&format!(
        "http://private-user:private-password@127.0.0.1:{port}"
    )))
    .await;

    assert_eq!(result["status"], "unavailable");
    assert_eq!(result["endpoint"], format!("127.0.0.1:{port}"));
    assert!(!result.to_string().contains("private-user"));
    assert!(!result.to_string().contains("private-password"));
}

#[tokio::test]
async fn network_safety_accepts_a_reachable_loopback_proxy() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    let result = network_safety_for_proxy(Some(&format!("socks5://127.0.0.1:{port}"))).await;

    assert_eq!(result["status"], "ok");
    assert_eq!(result["checked"], true);
    assert_eq!(result["endpoint"], format!("127.0.0.1:{port}"));
}
