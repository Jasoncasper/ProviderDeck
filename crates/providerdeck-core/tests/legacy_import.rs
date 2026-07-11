use std::fs;

use providerdeck_core::legacy_import::import_codexmate_config;

#[test]
fn imports_only_settings_and_routing_without_history() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join(".codex-session-delete");
    let destination = temp.path().join(".providerdeck");
    fs::create_dir_all(source.join("backups")).unwrap();
    fs::create_dir_all(source.join("sessions")).unwrap();
    fs::write(source.join("settings.json"), r#"{"debug_port":9229}"#).unwrap();
    fs::write(
        source.join("routing.toml"),
        "providers = []\nfallback_provider = \"openai\"\n",
    )
    .unwrap();
    fs::write(source.join("session-meta-backup.json"), "history").unwrap();
    fs::write(source.join("sessions/rollout.jsonl"), "history").unwrap();

    let result = import_codexmate_config(&source, &destination).unwrap();

    assert_eq!(result.imported, vec!["settings.json", "routing.toml"]);
    assert!(destination.join("settings.json").is_file());
    assert!(destination.join("routing.toml").is_file());
    assert!(!destination.join("session-meta-backup.json").exists());
    assert!(!destination.join("sessions").exists());
    assert!(!destination.join("backups").exists());
}

#[test]
fn existing_providerdeck_files_are_never_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("legacy");
    let destination = temp.path().join("current");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(source.join("settings.json"), "legacy").unwrap();
    fs::write(destination.join("settings.json"), "current").unwrap();

    let result = import_codexmate_config(&source, &destination).unwrap();

    assert!(result.imported.is_empty());
    assert_eq!(result.skipped, vec!["settings.json"]);
    assert_eq!(
        fs::read_to_string(destination.join("settings.json")).unwrap(),
        "current"
    );
}
