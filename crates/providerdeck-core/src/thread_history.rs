use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadHistorySafety {
    pub requires_compaction: bool,
    pub model: Option<String>,
}

#[derive(Debug, Default)]
struct TurnSafety {
    model: Option<String>,
    unsafe_reasoning: bool,
}

pub fn default_codex_sessions_dir() -> PathBuf {
    let codex_home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".codex")))
        .unwrap_or_else(|| PathBuf::from(".codex"));
    codex_home.join("sessions")
}

pub fn find_thread_rollout(
    sessions_dir: &Path,
    thread_id: &str,
) -> anyhow::Result<Option<PathBuf>> {
    validate_thread_id(thread_id)?;
    let suffix = format!("-{thread_id}.jsonl");
    let mut matches = Vec::new();
    collect_rollouts(sessions_dir, &suffix, 0, &mut matches)?;
    matches.sort_by(|left, right| {
        modified_at(left)
            .cmp(&modified_at(right))
            .then_with(|| left.cmp(right))
    });
    Ok(matches.pop())
}

pub fn analyze_rollout_history(path: &Path) -> anyhow::Result<ThreadHistorySafety> {
    let file = File::open(path)?;
    let mut turn_order = Vec::<String>::new();
    let mut turns = HashMap::<String, TurnSafety>::new();
    let mut unscoped_unsafe = false;

    for line in BufReader::new(file).lines() {
        let line = line?;
        let record = serde_json::from_str::<Value>(&line)
            .map_err(|error| anyhow::anyhow!("invalid rollout history: {error}"))?;
        let record_type = record.get("type").and_then(Value::as_str);
        let payload = record.get("payload").unwrap_or(&Value::Null);

        if record_type == Some("compacted") {
            turn_order.clear();
            turns.clear();
            unscoped_unsafe = payload
                .get("replacement_history")
                .and_then(Value::as_array)
                .is_some_and(|items| items.iter().any(is_unsafe_reasoning));
            continue;
        }

        if record_type == Some("turn_context") {
            if let Some(turn_id) = payload.get("turn_id").and_then(Value::as_str) {
                ensure_turn(&mut turn_order, &mut turns, turn_id);
                if let Some(model) = payload.get("model").and_then(Value::as_str) {
                    turns.entry(turn_id.to_string()).or_default().model = Some(model.to_string());
                }
            }
            continue;
        }

        if record_type == Some("event_msg") {
            match payload.get("type").and_then(Value::as_str) {
                Some("task_started") => {
                    if let Some(turn_id) = payload.get("turn_id").and_then(Value::as_str) {
                        ensure_turn(&mut turn_order, &mut turns, turn_id);
                    }
                }
                Some("thread_rolled_back") => {
                    let count = payload
                        .get("num_turns")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize;
                    for _ in 0..count {
                        if let Some(turn_id) = turn_order.pop() {
                            turns.remove(&turn_id);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }

        if record_type != Some("response_item") || !is_unsafe_reasoning(payload) {
            continue;
        }

        let turn_id = payload
            .pointer("/internal_chat_message_metadata_passthrough/turn_id")
            .and_then(Value::as_str)
            .or_else(|| payload.get("turn_id").and_then(Value::as_str));
        if let Some(turn_id) = turn_id {
            ensure_turn(&mut turn_order, &mut turns, turn_id);
            turns
                .entry(turn_id.to_string())
                .or_default()
                .unsafe_reasoning = true;
        } else {
            unscoped_unsafe = true;
        }
    }

    let unsafe_turn = turn_order
        .iter()
        .rev()
        .filter_map(|turn_id| turns.get(turn_id))
        .find(|turn| turn.unsafe_reasoning);
    Ok(ThreadHistorySafety {
        requires_compaction: unscoped_unsafe || unsafe_turn.is_some(),
        model: unsafe_turn.and_then(|turn| turn.model.clone()),
    })
}

pub fn thread_history_safety(thread_id: &str) -> anyhow::Result<(bool, ThreadHistorySafety)> {
    let Some(path) = find_thread_rollout(&default_codex_sessions_dir(), thread_id)? else {
        return Ok((
            false,
            ThreadHistorySafety {
                requires_compaction: false,
                model: None,
            },
        ));
    };
    Ok((true, analyze_rollout_history(&path)?))
}

fn validate_thread_id(thread_id: &str) -> anyhow::Result<()> {
    if thread_id.is_empty()
        || thread_id.len() > 128
        || thread_id == "."
        || thread_id == ".."
        || thread_id.contains("..")
        || !thread_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        anyhow::bail!("invalid thread id");
    }
    Ok(())
}

fn collect_rollouts(
    directory: &Path,
    suffix: &str,
    depth: usize,
    matches: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    if depth > 5 || !directory.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_rollouts(&path, suffix, depth + 1, matches)?;
        } else if file_type.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(suffix))
        {
            matches.push(path);
        }
    }
    Ok(())
}

fn modified_at(path: &Path) -> SystemTime {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn ensure_turn(
    turn_order: &mut Vec<String>,
    turns: &mut HashMap<String, TurnSafety>,
    turn_id: &str,
) {
    if !turns.contains_key(turn_id) {
        turn_order.push(turn_id.to_string());
        turns.insert(turn_id.to_string(), TurnSafety::default());
    }
}

fn is_unsafe_reasoning(item: &Value) -> bool {
    if item.get("type").and_then(Value::as_str) != Some("reasoning") {
        return false;
    }
    let has_id = item
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.is_empty());
    let has_encrypted_content = item
        .get("encrypted_content")
        .and_then(Value::as_str)
        .is_some_and(|content| !content.is_empty());
    has_id && !has_encrypted_content
}
