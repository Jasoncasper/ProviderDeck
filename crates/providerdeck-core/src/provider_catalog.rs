use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::router::config::SmartRouterConfig;

pub const SELECTION_PREFIX: &str = "providerdeck:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopedModel {
    pub provider_id: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDeckModel {
    pub selection: String,
    pub model: String,
    pub provider_id: String,
    pub source: ModelSource,
    pub display_name: String,
    pub description: String,
    pub supported_reasoning_efforts: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelSource {
    Official,
    Proxy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProvider {
    pub runtime_provider_id: String,
    pub name: String,
    pub base_url: String,
    pub bearer_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDeckCatalog {
    pub status: String,
    pub models: Vec<ProviderDeckModel>,
    pub providers: BTreeMap<String, RuntimeProvider>,
}

pub fn catalog_from_router_config(
    config: &SmartRouterConfig,
    helper_port: u16,
    bearer_token: &str,
) -> anyhow::Result<ProviderDeckCatalog> {
    let mut models = Vec::new();
    let mut providers = BTreeMap::new();
    for provider in config.providers.iter().filter(|provider| provider.enabled) {
        validate_provider_id(&provider.id)?;
        let model = if provider.target_model.trim().is_empty() {
            provider.id.trim()
        } else {
            provider.target_model.trim()
        };
        let selection = scoped_selection(&provider.id, model)?;
        models.push(ProviderDeckModel {
            selection,
            model: model.to_string(),
            provider_id: provider.id.clone(),
            source: ModelSource::Proxy,
            display_name: if provider.name.trim().is_empty() {
                model.to_string()
            } else {
                provider.name.clone()
            },
            description: format!("{} via ProviderDeck", provider.name.trim()),
            supported_reasoning_efforts: vec![
                "low".to_string(),
                "medium".to_string(),
                "high".to_string(),
            ],
        });
        providers.insert(
            provider.id.clone(),
            RuntimeProvider {
                runtime_provider_id: format!("providerdeck-{}", provider.id),
                name: provider.name.clone(),
                base_url: provider_helper_base_url(helper_port, &provider.id)?,
                bearer_token: bearer_token.to_string(),
            },
        );
    }
    Ok(ProviderDeckCatalog {
        status: "ok".to_string(),
        models,
        providers,
    })
}

pub fn catalog_from_path(
    path: &std::path::Path,
    helper_port: u16,
    bearer_token: &str,
) -> anyhow::Result<ProviderDeckCatalog> {
    let config = match std::fs::read_to_string(path) {
        Ok(contents) => toml::from_str::<SmartRouterConfig>(&contents)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => SmartRouterConfig::default(),
        Err(error) => return Err(error.into()),
    };
    catalog_from_router_config(
        &crate::router::config::normalize_router_config(config),
        helper_port,
        bearer_token,
    )
}

pub fn scoped_selection(provider_id: &str, model: &str) -> anyhow::Result<String> {
    validate_provider_id(provider_id)?;
    if model.trim().is_empty() {
        anyhow::bail!("model must not be empty");
    }
    Ok(format!("{SELECTION_PREFIX}{provider_id}:{model}"))
}

pub fn parse_scoped_selection(selection: &str) -> Option<ScopedModel> {
    let scoped = selection.strip_prefix(SELECTION_PREFIX)?;
    let (provider_id, model) = scoped.split_once(':')?;
    validate_provider_id(provider_id).ok()?;
    if model.trim().is_empty() {
        return None;
    }
    Some(ScopedModel {
        provider_id: provider_id.to_string(),
        model: model.to_string(),
    })
}

pub fn provider_helper_base_url(port: u16, provider_id: &str) -> anyhow::Result<String> {
    validate_provider_id(provider_id)?;
    Ok(format!("http://127.0.0.1:{port}/provider/{provider_id}/v1"))
}

pub fn validate_provider_id(provider_id: &str) -> anyhow::Result<()> {
    if provider_id.is_empty()
        || provider_id == "."
        || provider_id == ".."
        || provider_id.starts_with('.')
        || provider_id.ends_with('.')
        || !provider_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        anyhow::bail!("invalid provider id: {provider_id}");
    }
    Ok(())
}
