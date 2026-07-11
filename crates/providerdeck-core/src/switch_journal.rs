use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchPhase {
    Stable,
    Pending,
    Switching,
    RollingBack,
    Failed,
    RecoveryRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelBinding {
    pub model: String,
    pub provider_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchRecord {
    pub thread_id: String,
    pub original: ModelBinding,
    pub target: ModelBinding,
    pub phase: SwitchPhase,
    pub turn_status: String,
    pub updated_at_ms: u64,
    pub error: Option<String>,
}

impl SwitchRecord {
    pub fn begin(
        thread_id: impl Into<String>,
        original: ModelBinding,
        target: ModelBinding,
        turn_active: bool,
        updated_at_ms: u64,
    ) -> Self {
        Self {
            thread_id: thread_id.into(),
            original,
            target,
            phase: if turn_active {
                SwitchPhase::Pending
            } else {
                SwitchPhase::Switching
            },
            turn_status: if turn_active { "active" } else { "idle" }.to_string(),
            updated_at_ms,
            error: None,
        }
    }

    pub fn begin_switch(&mut self, updated_at_ms: u64) -> anyhow::Result<()> {
        if self.phase != SwitchPhase::Pending {
            anyhow::bail!("only a pending switch can begin");
        }
        self.phase = SwitchPhase::Switching;
        self.turn_status = "idle".to_string();
        self.updated_at_ms = updated_at_ms;
        self.error = None;
        Ok(())
    }

    pub fn begin_rollback(
        &mut self,
        error: impl Into<String>,
        updated_at_ms: u64,
    ) -> anyhow::Result<()> {
        if self.phase != SwitchPhase::Switching && self.phase != SwitchPhase::Failed {
            anyhow::bail!("rollback requires a switching or failed state");
        }
        self.phase = SwitchPhase::RollingBack;
        self.updated_at_ms = updated_at_ms;
        self.error = Some(error.into());
        Ok(())
    }

    pub fn require_recovery(
        &mut self,
        error: impl Into<String>,
        updated_at_ms: u64,
    ) -> anyhow::Result<()> {
        if self.phase != SwitchPhase::RollingBack {
            anyhow::bail!("manual recovery requires a rollback state");
        }
        self.phase = SwitchPhase::RecoveryRequired;
        self.updated_at_ms = updated_at_ms;
        self.error = Some(error.into());
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SwitchJournal {
    path: PathBuf,
}

impl SwitchJournal {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> anyhow::Result<Option<SwitchRecord>> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .with_context(|| format!("invalid switch journal {}", self.path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error)
                .with_context(|| format!("failed to read switch journal {}", self.path.display())),
        }
    }

    pub fn save(&self, record: &SwitchRecord) -> anyhow::Result<()> {
        self.write_bytes(&serde_json::to_vec_pretty(record)?)
    }

    pub fn save_value(&self, value: &serde_json::Value) -> anyhow::Result<()> {
        if contains_sensitive_key(value) {
            anyhow::bail!("switch journal must not contain credentials");
        }
        self.write_bytes(&serde_json::to_vec_pretty(value)?)
    }

    pub fn load_value(&self) -> anyhow::Result<Option<serde_json::Value>> {
        match fs::read(&self.path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write_bytes(&self, bytes: &[u8]) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp = temporary_path(&self.path);
        fs::write(&temp, bytes)?;
        fs::rename(&temp, &self.path)?;
        Ok(())
    }

    pub fn clear(&self) -> anyhow::Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

fn contains_sensitive_key(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => object.iter().any(|(key, value)| {
            let normalized = key
                .chars()
                .filter(|character| character.is_ascii_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>();
            matches!(
                normalized.as_str(),
                "apikey" | "bearertoken" | "authorization" | "secret"
            ) || contains_sensitive_key(value)
        }),
        serde_json::Value::Array(values) => values.iter().any(contains_sensitive_key),
        _ => false,
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "switch-journal".into());
    name.push(format!(".{}.tmp", std::process::id()));
    path.with_file_name(name)
}
