const RENDERER_SCRIPT: &str = include_str!("../../../assets/inject/renderer-inject.js");
pub const DIAGNOSTIC_BUILD_ID: &str = "diag-20260518-1";

pub fn renderer_script() -> &'static str {
    RENDERER_SCRIPT
}

pub fn injection_script(helper_port: u16) -> String {
    let catalog = crate::provider_catalog::catalog_from_path(
        &crate::paths::default_app_state_dir().join("routing.toml"),
        helper_port,
        crate::local_auth::runtime_bearer_token(),
    )
    .unwrap_or_else(|_| crate::provider_catalog::ProviderDeckCatalog {
        status: "failed".to_string(),
        models: Vec::new(),
        providers: Default::default(),
    });
    format!(
        "window.__PROVIDERDECK_BOOTSTRAP__ = {};\nwindow.__PROVIDERDECK_VERSION__ = {};\nwindow.__PROVIDERDECK_BUILD__ = {};\n{}",
        serde_json::to_string(&catalog).expect("catalog should serialize"),
        serde_json::to_string(crate::version::VERSION).expect("version should serialize"),
        serde_json::to_string(DIAGNOSTIC_BUILD_ID).expect("build id should serialize"),
        renderer_script(),
    )
}
