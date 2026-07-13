use std::path::Path;

use toml_edit::{DocumentMut, Item};

pub const RUNTIME_TOKEN_ENV: &str = "PROVIDERDECK_RUNTIME_TOKEN";

pub fn repair_providerdeck_selection(
    config_path: &Path,
    _routing_path: &Path,
    _helper_port: u16,
) -> anyhow::Result<bool> {
    let raw = match std::fs::read_to_string(config_path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let document = raw.parse::<DocumentMut>()?;
    let selection = document
        .get("model")
        .and_then(Item::as_str)
        .unwrap_or_default();
    let provider = document
        .get("model_provider")
        .and_then(Item::as_str)
        .unwrap_or_default();
    if !selection.starts_with(crate::provider_catalog::SELECTION_PREFIX)
        && !provider.starts_with("providerdeck-")
    {
        return Ok(false);
    }

    let repaired = remove_root_assignments(&raw, &["model", "model_provider"]);
    std::fs::write(config_path, repaired)?;
    Ok(true)
}

fn remove_root_assignments(raw: &str, keys: &[&str]) -> String {
    let mut in_root = true;
    raw.split_inclusive('\n')
        .filter(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with('[') {
                in_root = false;
            }
            !in_root
                || !keys.iter().any(|key| {
                    trimmed
                        .strip_prefix(key)
                        .is_some_and(|remainder| remainder.trim_start().starts_with('='))
                })
        })
        .collect()
}
