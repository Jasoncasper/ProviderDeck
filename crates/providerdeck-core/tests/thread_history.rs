use std::io::Write;

use providerdeck_core::thread_history::{analyze_rollout_history, find_thread_rollout};
use serde_json::{Value, json};

fn write_rollout(lines: &[Value]) -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    for line in lines {
        writeln!(file, "{}", serde_json::to_string(line).unwrap()).unwrap();
    }
    file
}

#[test]
fn proxy_reasoning_without_encrypted_content_requires_compaction() {
    let rollout = write_rollout(&[
        json!({
            "type": "turn_context",
            "payload": { "turn_id": "turn-proxy", "model": "vendor:model:v2" }
        }),
        json!({
            "type": "response_item",
            "payload": {
                "type": "reasoning",
                "id": "rs_resp_proxy",
                "summary": [{ "type": "summary_text", "text": "summary" }],
                "encrypted_content": null,
                "internal_chat_message_metadata_passthrough": { "turn_id": "turn-proxy" }
            }
        }),
    ]);

    let safety = analyze_rollout_history(rollout.path()).unwrap();

    assert!(safety.requires_compaction);
    assert_eq!(safety.model.as_deref(), Some("vendor:model:v2"));
}

#[test]
fn official_reasoning_with_encrypted_content_is_safe() {
    let rollout = write_rollout(&[
        json!({
            "type": "turn_context",
            "payload": { "turn_id": "turn-official", "model": "gpt-5.4" }
        }),
        json!({
            "type": "response_item",
            "payload": {
                "type": "reasoning",
                "id": "rs_official",
                "encrypted_content": "opaque-ciphertext",
                "internal_chat_message_metadata_passthrough": { "turn_id": "turn-official" }
            }
        }),
    ]);

    let safety = analyze_rollout_history(rollout.path()).unwrap();

    assert!(!safety.requires_compaction);
    assert_eq!(safety.model, None);
}

#[test]
fn successful_compaction_clears_unsafe_proxy_history() {
    let rollout = write_rollout(&[
        json!({
            "type": "turn_context",
            "payload": { "turn_id": "turn-proxy", "model": "vendor:model:v2" }
        }),
        json!({
            "type": "response_item",
            "payload": {
                "type": "reasoning",
                "id": "rs_resp_proxy",
                "encrypted_content": null,
                "internal_chat_message_metadata_passthrough": { "turn_id": "turn-proxy" }
            }
        }),
        json!({
            "type": "compacted",
            "payload": {
                "replacement_history": [
                    { "type": "message", "role": "user", "content": "kept" },
                    { "type": "compaction", "id": "cmp_1", "encrypted_content": "opaque" }
                ]
            }
        }),
    ]);

    let safety = analyze_rollout_history(rollout.path()).unwrap();

    assert!(!safety.requires_compaction);
    assert_eq!(safety.model, None);
}

#[test]
fn rollback_removes_unsafe_reasoning_from_the_discarded_turn() {
    let rollout = write_rollout(&[
        json!({
            "type": "event_msg",
            "payload": { "type": "task_started", "turn_id": "turn-proxy" }
        }),
        json!({
            "type": "turn_context",
            "payload": { "turn_id": "turn-proxy", "model": "vendor:model:v2" }
        }),
        json!({
            "type": "response_item",
            "payload": {
                "type": "reasoning",
                "id": "rs_resp_proxy",
                "encrypted_content": null,
                "internal_chat_message_metadata_passthrough": { "turn_id": "turn-proxy" }
            }
        }),
        json!({
            "type": "event_msg",
            "payload": { "type": "thread_rolled_back", "num_turns": 1 }
        }),
    ]);

    let safety = analyze_rollout_history(rollout.path()).unwrap();

    assert!(!safety.requires_compaction);
    assert_eq!(safety.model, None);
}

#[test]
fn malformed_rollout_fails_closed() {
    let mut rollout = tempfile::NamedTempFile::new().unwrap();
    writeln!(rollout, "{{not-json").unwrap();

    let error = analyze_rollout_history(rollout.path()).unwrap_err();

    assert!(error.to_string().contains("invalid rollout history"));
}

#[test]
fn rollout_lookup_matches_only_the_requested_thread_id() {
    let root = tempfile::tempdir().unwrap();
    let day = root.path().join("2026/07/14");
    std::fs::create_dir_all(&day).unwrap();
    let wanted = day.join("rollout-2026-07-14T01-00-00-019f5c7b-0a7f-7a30-9871-28c5b7c80641.jsonl");
    std::fs::write(&wanted, "").unwrap();
    std::fs::write(
        day.join("rollout-2026-07-14T01-00-00-019f5c7b-0a7f-7a30-9871-28c5b7c80642.jsonl"),
        "",
    )
    .unwrap();

    let found = find_thread_rollout(root.path(), "019f5c7b-0a7f-7a30-9871-28c5b7c80641")
        .unwrap()
        .unwrap();

    assert_eq!(found, wanted);
    assert!(find_thread_rollout(root.path(), "../../routing.toml").is_err());
}
