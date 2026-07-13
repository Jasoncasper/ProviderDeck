use std::fs;

use providerdeck_core::codex_config::repair_providerdeck_selection;

#[test]
fn clears_persisted_virtual_model_before_codex_starts() {
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("config.toml");
    let routing_path = temp.path().join("routing.toml");
    fs::write(
        &config_path,
        r#"# keep this comment
model_provider = "openai"
model = "providerdeck:deepseek-v4-flash:deepseek-chat"
"#,
    )
    .unwrap();
    fs::write(
        &routing_path,
        r#"
[[providers]]
id = "deepseek-v4-flash"
name = "DeepSeek V4 Flash"
base_url = "https://api.example.test"
api_key = "upstream-secret"
protocol = "chat_completions"
enabled = true
target_model = "deepseek-chat"
"#,
    )
    .unwrap();

    let repaired = repair_providerdeck_selection(&config_path, &routing_path, 57421).unwrap();

    assert!(repaired);
    let raw = fs::read_to_string(&config_path).unwrap();
    assert!(raw.contains("# keep this comment"));
    assert!(!raw.contains("upstream-secret"));
    let config: toml::Value = toml::from_str(&raw).unwrap();
    assert!(config.get("model").is_none());
    assert!(config.get("model_provider").is_none());
}

#[test]
fn clears_persisted_providerdeck_provider_with_real_model_name() {
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("config.toml");
    let routing_path = temp.path().join("routing.toml");
    fs::write(
        &config_path,
        r#"model = "deepseek-chat"
model_provider = "providerdeck-deepseek-v4-flash"

[features]
multi_agent = true
"#,
    )
    .unwrap();

    let repaired = repair_providerdeck_selection(&config_path, &routing_path, 57421).unwrap();

    assert!(repaired);
    let raw = fs::read_to_string(&config_path).unwrap();
    let config: toml::Value = toml::from_str(&raw).unwrap();
    assert!(config.get("model").is_none());
    assert!(config.get("model_provider").is_none());
    assert_eq!(config["features"]["multi_agent"].as_bool(), Some(true));
}
