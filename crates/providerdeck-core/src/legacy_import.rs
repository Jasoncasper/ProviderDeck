use std::fs;
use std::path::Path;

use serde::Serialize;

const IMPORTABLE_FILES: [&str; 2] = ["settings.json", "routing.toml"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LegacyImportResult {
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
}

/// Copies only ProviderDeck-compatible configuration. Codex sessions and backups
/// are deliberately outside this import boundary.
pub fn import_codexmate_config(
    source_dir: &Path,
    destination_dir: &Path,
) -> anyhow::Result<LegacyImportResult> {
    let mut result = LegacyImportResult::default();

    for file_name in IMPORTABLE_FILES {
        let source = source_dir.join(file_name);
        if !source.is_file() {
            continue;
        }

        let destination = destination_dir.join(file_name);
        if destination.exists() {
            result.skipped.push(file_name.to_string());
            continue;
        }

        fs::create_dir_all(destination_dir)?;
        if file_name == "settings.json" {
            let raw = fs::read_to_string(&source)?;
            let filtered = crate::settings::importable_settings_value(serde_json::from_str(&raw)?);
            fs::write(destination, serde_json::to_vec_pretty(&filtered)?)?;
        } else {
            let raw = fs::read_to_string(&source)?;
            let config: crate::router::SmartRouterConfig = toml::from_str(&raw)?;
            let config = crate::router::normalize_router_config(config);
            fs::write(destination, toml::to_string_pretty(&config)?)?;
        }
        result.imported.push(file_name.to_string());
    }

    Ok(result)
}
