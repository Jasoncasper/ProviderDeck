use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BackendSettings {
    #[serde(rename = "codexAppPath", default)]
    pub codex_app_path: String,
    #[serde(rename = "codexExtraArgs", default)]
    pub codex_extra_args: Vec<String>,
    #[serde(rename = "enhancementsEnabled", default = "default_true")]
    pub enhancements_enabled: bool,
}

impl Default for BackendSettings {
    fn default() -> Self {
        Self {
            codex_app_path: String::new(),
            codex_extra_args: Vec::new(),
            enhancements_enabled: true,
        }
    }
}

pub fn default_true() -> bool {
    true
}

pub fn normalize_codex_extra_args(args: &[String]) -> Vec<String> {
    args.iter()
        .map(|arg| arg.trim())
        .filter(|arg| !arg.is_empty())
        .map(str::to_string)
        .collect()
}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new(crate::paths::default_settings_path())
    }
}

impl SettingsStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> anyhow::Result<BackendSettings> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BackendSettings::default());
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to read settings {}", self.path.display()));
            }
        };
        Ok(serde_json::from_str(&contents).unwrap_or_default())
    }

    pub fn save(&self, settings: &BackendSettings) -> anyhow::Result<()> {
        let mut settings = settings.clone();
        settings.codex_extra_args = normalize_codex_extra_args(&settings.codex_extra_args);
        atomic_write(&self.path, &serde_json::to_vec_pretty(&settings)?)
    }

    pub fn update(&self, payload: Value) -> anyhow::Result<BackendSettings> {
        let mut settings = self.load()?;
        let Value::Object(payload) = payload else {
            return Ok(settings);
        };
        if let Some(value) = payload.get("codexAppPath").and_then(Value::as_str) {
            settings.codex_app_path = value.to_string();
        }
        if let Some(value) = payload.get("codexExtraArgs").and_then(Value::as_array) {
            settings.codex_extra_args = normalize_codex_extra_args(
                &value
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
            );
        }
        if let Some(value) = payload.get("enhancementsEnabled").and_then(Value::as_bool) {
            settings.enhancements_enabled = value;
        }
        self.save(&settings)?;
        Ok(settings)
    }
}

pub fn importable_settings_value(value: Value) -> Value {
    let source = value.as_object();
    let mut output = Map::new();
    if let Some(value) = source
        .and_then(|object| object.get("codexAppPath"))
        .and_then(Value::as_str)
    {
        output.insert("codexAppPath".to_string(), Value::String(value.to_string()));
    }
    if let Some(value) = source
        .and_then(|object| object.get("codexExtraArgs"))
        .and_then(Value::as_array)
    {
        output.insert(
            "codexExtraArgs".to_string(),
            Value::Array(
                value
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|item| Value::String(item.to_string()))
                    .collect(),
            ),
        );
    }
    output.insert("enhancementsEnabled".to_string(), Value::Bool(true));
    Value::Object(output)
}

pub(crate) fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    let mut temp = path.as_os_str().to_os_string();
    temp.push(format!(".{}.tmp", std::process::id()));
    let temp = PathBuf::from(temp);
    fs::write(&temp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(temp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_enable_runtime_enhancements() {
        assert!(BackendSettings::default().enhancements_enabled);
    }

    #[test]
    fn import_whitelist_drops_legacy_mode_and_context_fields() {
        let value = importable_settings_value(serde_json::json!({
            "codexAppPath": "/Applications/ChatGPT.app",
            "launchMode": "relay",
            "relayApiKey": "secret",
            "relayContextConfigContents": "[mcp_servers.test]"
        }));
        assert_eq!(value["codexAppPath"], "/Applications/ChatGPT.app");
        assert!(value.get("launchMode").is_none());
        assert!(value.get("relayApiKey").is_none());
        assert!(value.get("relayContextConfigContents").is_none());
    }

    #[test]
    fn store_round_trip_normalizes_extra_args() {
        let temp = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(temp.path().join("settings.json"));
        store
            .save(&BackendSettings {
                codex_app_path: "ChatGPT.app".into(),
                codex_extra_args: vec![" --flag ".into(), "".into()],
                enhancements_enabled: true,
            })
            .unwrap();
        assert_eq!(store.load().unwrap().codex_extra_args, vec!["--flag"]);
    }
}
