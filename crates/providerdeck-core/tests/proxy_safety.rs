use std::collections::HashMap;
use std::io::{Read, Write};
use std::time::Duration;

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

#[tokio::test]
async fn network_safety_rejects_a_loopback_http_proxy_without_upstream_connectivity() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let proxy = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut request = [0u8; 1024];
        let read = stream.read(&mut request).unwrap_or_default();
        assert!(
            String::from_utf8_lossy(&request[..read]).starts_with("CONNECT chatgpt.com:443 "),
            "network safety should validate the proxy's upstream tunnel"
        );
        stream
            .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });

    let result = network_safety_for_proxy(Some(&format!("http://127.0.0.1:{port}"))).await;
    proxy.join().unwrap();

    assert_eq!(result["status"], "unavailable");
    assert_eq!(result["endpoint"], format!("127.0.0.1:{port}"));
    assert_eq!(result["checked"], true);
    assert!(result["message"].as_str().unwrap().contains("上游"));
}

#[tokio::test]
async fn network_safety_rejects_a_proxy_tunnel_when_upstream_tls_stalls() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let proxy = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut request = [0u8; 1024];
        let read = stream.read(&mut request).unwrap_or_default();
        assert!(String::from_utf8_lossy(&request[..read]).starts_with("CONNECT chatgpt.com:443 "));
        stream
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .unwrap();
        std::thread::sleep(Duration::from_secs(3));
    });

    let result = network_safety_for_proxy(Some(&format!("http://127.0.0.1:{port}"))).await;
    proxy.join().unwrap();

    assert_eq!(result["status"], "unavailable");
    assert!(result["message"].as_str().unwrap().contains("上游"));
}
