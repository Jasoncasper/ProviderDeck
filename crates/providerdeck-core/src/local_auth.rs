use std::sync::OnceLock;

use base64::Engine;

static RUNTIME_BEARER_TOKEN: OnceLock<String> = OnceLock::new();

pub fn runtime_bearer_token() -> &'static str {
    RUNTIME_BEARER_TOKEN.get_or_init(|| {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>())
    })
}

pub fn authorization_matches(request: &str, expected: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    request.lines().any(|line| {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        name.trim().eq_ignore_ascii_case("authorization")
            && value.trim() == format!("Bearer {expected}")
    })
}
