// Routing-specific types

export type ProviderProtocol = "responses" | "chat_completions" | "anthropic" | "custom";

export interface SmartProvider {
  id: string;
  name: string;
  base_url: string;
  api_key: string;
  protocol: ProviderProtocol;
  enabled: boolean;
  supports_vision: boolean;
  use_full_url: boolean;
  target_model: string;
  model_pattern: string;
  builtin: boolean;
  user_agent: string;
  max_context: number;
  supports_large_context: boolean;
  max_concurrent: number;
}

export interface SmartRouterConfig {
  providers: SmartProvider[];
  vision_fallback_model: string;
  fallback_provider: string;
  fallback: { enabled: boolean; max_retries: number; retry_delay_ms: number };
  image_auto_compress: boolean;
  image_max_size_kb: number;
  image_max_dimension: number;
}

export interface RoutingConfigPayload {
  config: SmartRouterConfig;
  config_path: string;
}
