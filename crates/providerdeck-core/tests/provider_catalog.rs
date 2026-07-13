use providerdeck_core::provider_catalog::{
    catalog_from_router_config, parse_scoped_selection, provider_helper_base_url,
    provider_models_payload, runtime_provider_id, scoped_selection,
};
use providerdeck_core::router::config::{ProviderProtocol, SmartProvider, SmartRouterConfig};

fn provider(id: &str, enabled: bool, target_model: &str, api_key: &str) -> SmartProvider {
    SmartProvider {
        id: id.to_string(),
        name: id.replace('_', " "),
        base_url: "https://api.example.test/v1".to_string(),
        api_key: api_key.to_string(),
        protocol: ProviderProtocol::ChatCompletions,
        enabled,
        supports_vision: false,
        use_full_url: false,
        target_model: target_model.to_string(),
        model_pattern: String::new(),
        builtin: false,
        user_agent: String::new(),
        max_context: 0,
        supports_large_context: false,
        max_concurrent: 2,
    }
}

#[test]
fn scoped_selection_preserves_model_names_with_colons() {
    let selection = scoped_selection("team_proxy", "vendor:model:v2").unwrap();

    assert_eq!(selection, "providerdeck:team_proxy:vendor:model:v2");
    let parsed = parse_scoped_selection(&selection).unwrap();
    assert_eq!(parsed.provider_id, "team_proxy");
    assert_eq!(parsed.model, "vendor:model:v2");
}

#[test]
fn official_models_are_not_treated_as_scoped_selections() {
    assert!(parse_scoped_selection("gpt-5.4").is_none());
}

#[test]
fn provider_ids_reject_path_and_selection_delimiters() {
    for provider_id in ["bad:id", "../escape", "has space", ""] {
        assert!(scoped_selection(provider_id, "model").is_err());
        assert!(provider_helper_base_url(57322, provider_id).is_err());
    }
}

#[test]
fn helper_base_url_is_scoped_to_one_provider() {
    assert_eq!(
        provider_helper_base_url(57322, "team_proxy").unwrap(),
        "http://127.0.0.1:57322/provider/team_proxy/v1"
    );
}

#[test]
fn runtime_provider_ids_encode_dots_without_colliding_with_reserved_raw_ids() {
    assert_eq!(
        runtime_provider_id("deepseek-v4-pro").unwrap(),
        "providerdeck-deepseek-v4-pro"
    );
    assert_eq!(
        runtime_provider_id("glm-5.2").unwrap(),
        "providerdeck-pdhex-676c6d2d352e32"
    );
    assert_eq!(
        runtime_provider_id("pdhex-676c6d2d352e32").unwrap(),
        "providerdeck-pdhex-70646865782d3637366336643264333532653332"
    );
}

#[test]
fn runtime_catalog_uses_toml_safe_runtime_provider_id_for_dotted_provider() {
    let config = SmartRouterConfig {
        providers: vec![provider("glm-5.2", true, "glm-5.2", "upstream-secret")],
        ..SmartRouterConfig::default()
    };

    let catalog = catalog_from_router_config(&config, 57322, "runtime-token").unwrap();

    assert_eq!(
        catalog.providers["glm-5.2"].runtime_provider_id,
        "providerdeck-pdhex-676c6d2d352e32"
    );
    assert!(
        !catalog.providers["glm-5.2"]
            .runtime_provider_id
            .contains('.')
    );
}

#[test]
fn runtime_catalog_excludes_disabled_providers_and_api_keys() {
    let enabled = provider("team_proxy", true, "vendor:model:v2", "upstream-secret");
    let disabled = provider("disabled", false, "disabled-model", "disabled-secret");
    let config = SmartRouterConfig {
        providers: vec![enabled, disabled],
        ..SmartRouterConfig::default()
    };

    let catalog = catalog_from_router_config(&config, 57322, "runtime-token").unwrap();
    let serialized = serde_json::to_string(&catalog).unwrap();

    assert_eq!(catalog.models.len(), 1);
    assert_eq!(
        catalog.models[0].selection,
        "providerdeck:team_proxy:vendor:model:v2"
    );
    assert_eq!(
        catalog.providers["team_proxy"].runtime_provider_id,
        "providerdeck-team_proxy"
    );
    assert!(!catalog.providers.contains_key("disabled"));
    assert!(!serialized.contains("upstream-secret"));
    assert!(!serialized.contains("disabled-secret"));
}

#[test]
fn scoped_provider_models_payload_matches_codex_catalog_contract() {
    let config = SmartRouterConfig {
        providers: vec![provider(
            "team_proxy",
            true,
            "vendor:model:v2",
            "upstream-secret",
        )],
        ..SmartRouterConfig::default()
    };
    let catalog = catalog_from_router_config(&config, 57322, "runtime-token").unwrap();

    let payload = provider_models_payload(&catalog, "team_proxy").unwrap();

    assert_eq!(payload, serde_json::json!({ "models": [] }));
}
