//! 路由引擎 — 核心路由决策逻辑

use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::config::*;

/// 路由决策结果
#[derive(Debug, Clone)]
pub struct RouteDecision {
    /// 选中的模型配置
    pub provider: SmartProvider,
    /// 目标模型名
    pub target_model: String,
    /// 决策说明
    pub rule_name: String,
}

/// 路由引擎
pub struct RouterEngine {
    config: Arc<RwLock<SmartRouterConfig>>,
}

impl RouterEngine {
    /// 创建新的路由引擎
    pub fn new(config: SmartRouterConfig) -> Arc<Self> {
        Arc::new(Self {
            config: Arc::new(RwLock::new(config)),
        })
    }

    /// 从配置文件加载并创建路由引擎
    pub fn load_from_file(path: &Path) -> anyhow::Result<Arc<Self>> {
        let content = std::fs::read_to_string(path)?;
        let config: SmartRouterConfig = normalize_router_config(toml::from_str(&content)?);
        Ok(Self::new(config))
    }

    /// 获取当前配置
    pub async fn get_config(&self) -> SmartRouterConfig {
        self.config.read().await.clone()
    }

    /// 更新配置
    pub async fn update_config(&self, config: SmartRouterConfig) {
        let mut cfg = self.config.write().await;
        *cfg = config;
    }

    /// 核心路由方法：按模型名查找配置
    pub async fn route(&self, request: &RequestContext) -> anyhow::Result<RouteDecision> {
        let config = normalize_router_config(self.config.read().await.clone());

        // 辅助：解析实际发给上游的模型名
        fn resolve_target(provider: &SmartProvider, fallback: &str) -> String {
            if !provider.target_model.trim().is_empty() {
                provider.target_model.trim().to_string()
            } else {
                fallback.to_string()
            }
        }

        // 收集路由决策，稍后统一应用 vision fallback
        let mut decision: RouteDecision;

        // 1. ProviderDeck 托管模型使用 provider_id:model_id
        if let Some((provider_id, target_model)) = split_scoped_model(&request.model) {
            if let Some(p) = config
                .providers
                .iter()
                .find(|p| p.enabled && p.id == provider_id)
            {
                decision = RouteDecision {
                    target_model: resolve_target(p, target_model),
                    provider: p.clone(),
                    rule_name: "scoped-model-match".to_string(),
                };
            } else {
                anyhow::bail!("Scoped provider not found: {provider_id}");
            }
        } else
        // 2. 按模型名精确匹配
        if let Some(p) = config
            .providers
            .iter()
            .find(|p| p.enabled && p.id == request.model)
        {
            decision = RouteDecision {
                target_model: resolve_target(p, &request.model),
                provider: p.clone(),
                rule_name: "model-match".to_string(),
            };
        } else
        // 4. 按 model_pattern 通配符匹配
        if let Some(p) = config
            .providers
            .iter()
            .find(|p| p.enabled && provider_matches_model(p, &request.model))
        {
            decision = RouteDecision {
                target_model: resolve_target(p, &p.id),
                provider: p.clone(),
                rule_name: "pattern-match".to_string(),
            };
        } else
        // 5. 使用配置的 fallback_provider，否则取第一个启用的 provider
        {
            let p = if !config.fallback_provider.is_empty() {
                config
                    .providers
                    .iter()
                    .find(|p| p.enabled && p.id == config.fallback_provider)
                    .or_else(|| config.providers.iter().find(|p| p.enabled))
            } else {
                config.providers.iter().find(|p| p.enabled)
            };
            let p = p.ok_or_else(|| anyhow::anyhow!("No enabled models"))?;
            let is_fallback = !config.fallback_provider.is_empty()
                && config
                    .providers
                    .iter()
                    .any(|fp| fp.enabled && fp.id == config.fallback_provider);
            decision = RouteDecision {
                target_model: resolve_target(p, &p.id),
                provider: p.clone(),
                rule_name: if is_fallback {
                    "fallback-provider"
                } else {
                    "default"
                }
                .to_string(),
            };
        }

        // 3. 统一图片多模态回退：图片请求且 provider 不支持多模态 → 切换到 vision_fallback_model
        if request.has_image
            && !decision.provider.supports_vision
            && !config.vision_fallback_model.is_empty()
        {
            if let Some(fallback) = config
                .providers
                .iter()
                .find(|fp| fp.enabled && fp.id == config.vision_fallback_model)
            {
                decision = RouteDecision {
                    target_model: resolve_target(fallback, &config.vision_fallback_model),
                    provider: fallback.clone(),
                    rule_name: "vision-fallback".to_string(),
                };
            }
        }

        Ok(decision)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn provider(id: &str, builtin: bool, pattern: &str) -> SmartProvider {
        SmartProvider {
            id: id.to_string(),
            name: id.to_string(),
            base_url: format!("https://{id}.example.test/v1"),
            api_key: String::new(),
            protocol: ProviderProtocol::Responses,
            enabled: true,
            supports_vision: true,
            use_full_url: false,
            target_model: String::new(),
            model_pattern: pattern.to_string(),
            builtin,
            user_agent: String::new(),
            max_context: 0,
            supports_large_context: false,
            max_concurrent: 2,
        }
    }

    #[test]
    fn model_pattern_supports_multiple_official_model_families() {
        assert!(model_matches_pattern("gpt-5", DEFAULT_OPENAI_MODEL_PATTERN));
        assert!(model_matches_pattern(
            "gpt-5.5",
            DEFAULT_OPENAI_MODEL_PATTERN
        ));
        assert!(model_matches_pattern(
            "gpt5.5",
            DEFAULT_OPENAI_MODEL_PATTERN
        ));
        assert!(model_matches_pattern("o3", DEFAULT_OPENAI_MODEL_PATTERN));
        assert!(model_matches_pattern(
            "o4-mini",
            DEFAULT_OPENAI_MODEL_PATTERN
        ));
        assert!(model_matches_pattern(
            "codex-mini-latest",
            DEFAULT_OPENAI_MODEL_PATTERN
        ));
        assert!(!model_matches_pattern(
            "qwen3-coder",
            DEFAULT_OPENAI_MODEL_PATTERN
        ));
    }

    #[test]
    fn normalize_drops_all_openai_providers() {
        let config = SmartRouterConfig {
            providers: vec![
                provider(INTERNAL_OPENAI_PROVIDER_ID, true, ""),
                provider(INTERNAL_OPENAI_PROVIDER_ID, false, ""),
                provider("deepseek-v4-pro", false, ""),
            ],
            vision_fallback_model: String::new(),
            fallback_provider: String::new(),
            fallback: FallbackConfig::default(),
            image_auto_compress: true,
            image_max_dimension: 1024,
            image_max_size_kb: 100,
        };

        let normalized = normalize_router_config(config);

        assert_eq!(normalized.providers.len(), 1);
        assert_eq!(normalized.providers[0].id, "deepseek-v4-pro");
        assert!(normalized.fallback_provider.is_empty());
        assert!(normalized.vision_fallback_model.is_empty());
    }

    #[tokio::test]
    async fn scoped_model_routes_to_provider_and_uses_unscoped_target_model() {
        let config = SmartRouterConfig {
            providers: vec![provider("deepseek-v4-pro", false, "")],
            vision_fallback_model: String::new(),
            fallback_provider: String::new(),
            fallback: FallbackConfig::default(),
            image_auto_compress: true,
            image_max_dimension: 1024,
            image_max_size_kb: 100,
        };
        let router = RouterEngine::new(config);

        let decision = router
            .route(&RequestContext {
                model: "deepseek-v4-pro:gpt-5.4-mini".to_string(),
                has_image: false,
                has_tools: false,
            })
            .await
            .unwrap();

        assert_eq!(decision.provider.id, "deepseek-v4-pro");
        assert_eq!(decision.target_model, "gpt-5.4-mini");
        assert_eq!(decision.rule_name, "scoped-model-match");
    }

    #[tokio::test]
    async fn scoped_and_raw_model_names_route_to_existing_provider() {
        let mut deepseek = provider("deepseek-v4-flash", false, "");
        deepseek.target_model = "deepseek-chat".to_string();
        let config = SmartRouterConfig {
            providers: vec![deepseek],
            vision_fallback_model: String::new(),
            fallback_provider: String::new(),
            fallback: FallbackConfig::default(),
            image_auto_compress: true,
            image_max_dimension: 1024,
            image_max_size_kb: 100,
        };
        let router = RouterEngine::new(config);

        let scoped = router
            .route(&RequestContext {
                model: "deepseek-v4-flash:deepseek-chat".to_string(),
                has_image: false,
                has_tools: false,
            })
            .await
            .unwrap();
        let raw = router
            .route(&RequestContext {
                model: "deepseek-v4-flash".to_string(),
                has_image: false,
                has_tools: false,
            })
            .await
            .unwrap();

        assert_eq!(scoped.provider.id, "deepseek-v4-flash");
        assert_eq!(scoped.target_model, "deepseek-chat");
        assert_eq!(scoped.rule_name, "scoped-model-match");
        assert_eq!(raw.provider.id, "deepseek-v4-flash");
        assert_eq!(raw.target_model, "deepseek-chat");
        assert_eq!(raw.rule_name, "model-match");
    }
}
