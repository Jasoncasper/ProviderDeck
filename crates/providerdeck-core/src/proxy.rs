use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde_json::{Value, json};

static CODEX_PROXY_SNAPSHOT: OnceLock<Mutex<Option<String>>> = OnceLock::new();

pub fn has_proxy_environment(env: &HashMap<String, String>) -> bool {
    [
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "ALL_PROXY",
        "https_proxy",
        "http_proxy",
        "all_proxy",
    ]
    .into_iter()
    .any(|name| env.get(name).is_some_and(|value| !value.is_empty()))
}

pub fn detect_system_proxy() -> Option<String> {
    platform_system_proxy()
}

pub fn proxy_url_from_environment(env: &HashMap<String, String>) -> Option<String> {
    [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ]
    .into_iter()
    .find_map(|name| env.get(name).filter(|value| !value.is_empty()).cloned())
}

pub fn set_codex_proxy_snapshot(proxy: Option<String>) {
    let configured = proxy.as_ref().is_some_and(|value| !value.is_empty());
    let endpoint = proxy
        .as_deref()
        .and_then(parse_proxy_endpoint)
        .map(|endpoint| endpoint.label());
    let snapshot = CODEX_PROXY_SNAPSHOT.get_or_init(|| Mutex::new(None));
    *snapshot.lock().expect("Codex proxy snapshot lock poisoned") = proxy;
    let _ = crate::diagnostic_log::append_diagnostic_log(
        "codex.proxy_snapshot",
        json!({
            "configured": configured,
            "endpoint": endpoint,
        }),
    );
}

pub async fn codex_network_safety() -> Value {
    let proxy = CODEX_PROXY_SNAPSHOT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("Codex proxy snapshot lock poisoned")
        .clone();
    let result = network_safety_for_proxy(proxy.as_deref()).await;
    if result.get("status").and_then(Value::as_str) == Some("unavailable") {
        let _ = crate::diagnostic_log::append_diagnostic_log(
            "codex.proxy_unavailable",
            json!({
                "endpoint": result.get("endpoint").cloned().unwrap_or(Value::Null),
            }),
        );
    }
    result
}

pub async fn wait_for_codex_network_ready() -> anyhow::Result<()> {
    let mut last_message = None;
    for attempt in 0..5 {
        let result = codex_network_safety().await;
        if result.get("status").and_then(Value::as_str) != Some("unavailable") {
            return Ok(());
        }
        last_message = result
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string);
        if attempt < 4 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    anyhow::bail!(
        "{}",
        last_message.unwrap_or_else(|| "Codex 网络代理当前不可用，请恢复 VPN 后重试".to_string())
    )
}

pub async fn network_safety_for_proxy(proxy: Option<&str>) -> Value {
    let Some(proxy) = proxy else {
        return json!({
            "status": "ok",
            "proxyConfigured": false,
            "checked": false,
        });
    };
    let Some(endpoint) = parse_proxy_endpoint(proxy) else {
        return json!({
            "status": "ok",
            "proxyConfigured": true,
            "checked": false,
        });
    };
    if !endpoint.is_loopback() {
        return json!({
            "status": "ok",
            "proxyConfigured": true,
            "checked": false,
        });
    }

    let endpoint_label = endpoint.label();
    if is_plain_http_proxy(proxy) {
        if http_proxy_upstream_available(proxy).await {
            return json!({
                "status": "ok",
                "proxyConfigured": true,
                "checked": true,
                "endpoint": endpoint_label,
            });
        }
        return json!({
            "status": "unavailable",
            "proxyConfigured": true,
            "checked": true,
            "endpoint": endpoint_label,
            "message": format!(
                "Codex 使用的本地代理 {endpoint_label} 无法建立 ChatGPT 上游连接，请检查代理节点后重试"
            ),
        });
    }

    let connection = tokio::time::timeout(
        Duration::from_millis(350),
        tokio::net::TcpStream::connect((endpoint.host.as_str(), endpoint.port)),
    )
    .await;
    let Ok(Ok(stream)) = connection else {
        return json!({
            "status": "unavailable",
            "proxyConfigured": true,
            "checked": true,
            "endpoint": endpoint_label,
            "message": format!(
                "Codex 使用的本地代理 {endpoint_label} 当前不可达，请先恢复代理连接后重试；如果代理端口已切换，请重启 Codex 以刷新配置"
            ),
        });
    };
    drop(stream);

    json!({
        "status": "ok",
        "proxyConfigured": true,
        "checked": true,
        "endpoint": endpoint_label,
    })
}

fn is_plain_http_proxy(proxy: &str) -> bool {
    proxy
        .split_once("://")
        .map(|(scheme, _)| scheme.eq_ignore_ascii_case("http"))
        .unwrap_or(true)
}

async fn http_proxy_upstream_available(proxy: &str) -> bool {
    let Ok(proxy) = reqwest::Proxy::all(proxy) else {
        return false;
    };
    let Ok(client) = reqwest::Client::builder()
        .proxy(proxy)
        // connect_timeout 覆盖经 HTTP 代理到目标的 TLS 握手；实测 chatgpt.com 经本地代理握手约 1.5s，
        // 1500ms 会稳定卡在边界并误报不可用，故放宽到 6s。
        .connect_timeout(Duration::from_millis(6000))
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return false;
    };
    client.head("https://chatgpt.com").send().await.is_ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProxyEndpoint {
    host: String,
    port: u16,
}

impl ProxyEndpoint {
    fn is_loopback(&self) -> bool {
        self.host.eq_ignore_ascii_case("localhost")
            || self
                .host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    }

    fn label(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

fn parse_proxy_endpoint(proxy: &str) -> Option<ProxyEndpoint> {
    let proxy = proxy.trim();
    if proxy.is_empty() {
        return None;
    }
    let (scheme, remainder) = proxy
        .split_once("://")
        .map(|(scheme, remainder)| (Some(scheme), remainder))
        .unwrap_or((None, proxy));
    let authority = remainder
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit_once('@')
        .map(|(_, authority)| authority)
        .unwrap_or(remainder.split(['/', '?', '#']).next().unwrap_or_default());
    let default_port = match scheme.map(str::to_ascii_lowercase).as_deref() {
        Some("https") => 443,
        Some("socks") | Some("socks5") | Some("socks5h") => 1080,
        _ => 80,
    };

    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let (host, suffix) = bracketed.split_once(']')?;
        let port = suffix
            .strip_prefix(':')
            .map(str::parse)
            .transpose()
            .ok()?
            .unwrap_or(default_port);
        (host, port)
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        (host, port.parse().ok()?)
    } else {
        (authority, default_port)
    };
    if host.is_empty() {
        return None;
    }
    Some(ProxyEndpoint {
        host: host.to_string(),
        port,
    })
}

fn normalize_proxy_url(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.contains("://") {
        return Some(value.to_string());
    }
    Some(format!("http://{value}"))
}

#[allow(dead_code)]
fn parse_windows_proxy_server(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if value.contains('=') {
        for wanted_scheme in ["https", "http"] {
            for entry in value.split(';').map(str::trim) {
                let Some((scheme, proxy)) = entry.split_once('=') else {
                    continue;
                };
                if scheme.eq_ignore_ascii_case(wanted_scheme) {
                    return normalize_proxy_url(proxy);
                }
            }
        }
        return None;
    }

    normalize_proxy_url(value)
}

#[cfg(any(test, target_os = "macos"))]
fn parse_macos_scutil_proxy(output: &str) -> Option<String> {
    let mut values = HashMap::new();
    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        values.insert(key.trim(), value.trim());
    }

    for (enable_key, host_key, port_key) in [
        ("HTTPEnable", "HTTPProxy", "HTTPPort"),
        ("HTTPSEnable", "HTTPSProxy", "HTTPSPort"),
    ] {
        if values.get(enable_key) != Some(&"1") {
            continue;
        }
        let host = values.get(host_key).copied().unwrap_or_default();
        let port = values.get(port_key).copied().unwrap_or_default();
        if !host.is_empty() && !port.is_empty() {
            return normalize_proxy_url(&format!("{host}:{port}"));
        }
    }

    None
}

#[cfg(windows)]
fn platform_system_proxy() -> Option<String> {
    windows_system_proxy()
}

#[cfg(target_os = "macos")]
fn platform_system_proxy() -> Option<String> {
    let output = std::process::Command::new("scutil")
        .arg("--proxy")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_macos_scutil_proxy(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform_system_proxy() -> Option<String> {
    None
}

#[cfg(windows)]
fn windows_system_proxy() -> Option<String> {
    use std::ffi::{OsStr, OsString};
    use std::iter::once;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_ROUTINE_FLAGS, RRF_RT_REG_DWORD, RRF_RT_REG_EXPAND_SZ,
        RRF_RT_REG_SZ, RegGetValueW,
    };
    use windows::core::PCWSTR;

    const INTERNET_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

    fn wide_null(value: impl AsRef<OsStr>) -> Vec<u16> {
        value.as_ref().encode_wide().chain(once(0)).collect()
    }

    fn read_dword(subkey: &str, name: &str) -> Option<u32> {
        let subkey = wide_null(subkey);
        let name = wide_null(name);
        let mut value = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_DWORD,
                None,
                Some((&mut value as *mut u32).cast()),
                Some(&mut size),
            )
        }
        .ok()
        .ok()?;
        Some(value)
    }

    fn read_string(subkey: &str, name: &str) -> Option<String> {
        let subkey = wide_null(subkey);
        let name = wide_null(name);
        let flags = REG_ROUTINE_FLAGS(RRF_RT_REG_SZ.0 | RRF_RT_REG_EXPAND_SZ.0);
        let mut size = 0u32;
        unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                PCWSTR(name.as_ptr()),
                flags,
                None,
                None,
                Some(&mut size),
            )
        }
        .ok()
        .ok()?;
        if size == 0 {
            return None;
        }

        let mut value = vec![0u16; (size as usize).div_ceil(2)];
        unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                PCWSTR(name.as_ptr()),
                flags,
                None,
                Some(value.as_mut_ptr().cast()),
                Some(&mut size),
            )
        }
        .ok()
        .ok()?;
        let len = value.iter().position(|ch| *ch == 0).unwrap_or(value.len());
        Some(
            OsString::from_wide(&value[..len])
                .to_string_lossy()
                .to_string(),
        )
    }

    if read_dword(INTERNET_SETTINGS, "ProxyEnable")? == 0 {
        return None;
    }

    parse_windows_proxy_server(&read_string(INTERNET_SETTINGS, "ProxyServer")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_proxy_server_prefers_https_scheme_entry() {
        assert_eq!(
            parse_windows_proxy_server(
                "http=proxy.example.test:8080;https=secure-proxy.example.test:8443"
            ),
            Some("http://secure-proxy.example.test:8443".to_string())
        );
    }

    #[test]
    fn windows_proxy_server_prefixes_plain_host() {
        assert_eq!(
            parse_windows_proxy_server("proxy.example.test:8080"),
            Some("http://proxy.example.test:8080".to_string())
        );
    }

    #[test]
    fn macos_scutil_proxy_parses_enabled_http_proxy() {
        let output = r#"
<dictionary> {
  HTTPEnable : 1
  HTTPPort : 8080
  HTTPProxy : proxy.example.test
  HTTPSEnable : 0
}
"#;

        assert_eq!(
            parse_macos_scutil_proxy(output),
            Some("http://proxy.example.test:8080".to_string())
        );
    }

    #[test]
    fn macos_scutil_proxy_ignores_disabled_proxy() {
        let output = r#"
<dictionary> {
  HTTPEnable : 0
  HTTPPort : 8080
  HTTPProxy : proxy.example.test
}
"#;

        assert_eq!(parse_macos_scutil_proxy(output), None);
    }
}
