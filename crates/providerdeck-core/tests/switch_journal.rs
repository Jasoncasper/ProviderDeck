use providerdeck_core::switch_journal::{ModelBinding, SwitchJournal, SwitchPhase, SwitchRecord};

fn binding(model: &str, provider_id: &str) -> ModelBinding {
    ModelBinding {
        model: model.to_string(),
        provider_id: provider_id.to_string(),
    }
}

#[test]
fn active_turn_starts_as_pending_and_idle_turn_starts_as_switching() {
    let active = SwitchRecord::begin(
        "thread-1",
        binding("gpt-5.4", "openai"),
        binding("deepseek-v4", "team_proxy"),
        true,
        100,
    );
    let idle = SwitchRecord::begin(
        "thread-1",
        binding("gpt-5.4", "openai"),
        binding("deepseek-v4", "team_proxy"),
        false,
        100,
    );

    assert_eq!(active.phase, SwitchPhase::Pending);
    assert_eq!(idle.phase, SwitchPhase::Switching);
}

#[test]
fn failed_target_can_enter_rollback_then_recovery_required() {
    let mut record = SwitchRecord::begin(
        "thread-1",
        binding("gpt-5.4", "openai"),
        binding("deepseek-v4", "team_proxy"),
        false,
        100,
    );

    record.begin_rollback("target unavailable", 101).unwrap();
    assert_eq!(record.phase, SwitchPhase::RollingBack);
    assert_eq!(record.error.as_deref(), Some("target unavailable"));

    record
        .require_recovery("rollback unavailable", 102)
        .unwrap();
    assert_eq!(record.phase, SwitchPhase::RecoveryRequired);
    assert_eq!(record.error.as_deref(), Some("rollback unavailable"));
}

#[test]
fn journal_round_trip_is_atomic_and_contains_no_secret_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("switch-journal.json");
    let journal = SwitchJournal::new(path.clone());
    let record = SwitchRecord::begin(
        "thread-1",
        binding("gpt-5.4", "openai"),
        binding("deepseek-v4", "team_proxy"),
        false,
        100,
    );

    journal.save(&record).unwrap();
    assert_eq!(journal.load().unwrap(), Some(record));
    let raw = std::fs::read_to_string(path).unwrap();
    assert!(!raw.contains("api_key"));
    assert!(!raw.contains("bearer"));

    journal.clear().unwrap();
    assert_eq!(journal.load().unwrap(), None);
}

#[test]
fn raw_renderer_journal_rejects_secret_fields() {
    let dir = tempfile::tempdir().unwrap();
    let journal = SwitchJournal::new(dir.path().join("switch-journal.json"));

    assert!(
        journal
            .save_value(&serde_json::json!({
                "phase": "switching",
                "threadId": "thread-1",
                "target": { "model": "model", "providerId": "proxy" }
            }))
            .is_ok()
    );
    assert!(
        journal
            .save_value(&serde_json::json!({
                "phase": "switching",
                "apiKey": "must-not-persist"
            }))
            .is_err()
    );
    assert!(
        journal
            .save_value(&serde_json::json!({
                "provider": { "bearerToken": "must-not-persist" }
            }))
            .is_err()
    );
}
