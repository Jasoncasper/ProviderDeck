# Changelog

## v1.0.0 (2026-07-14)

### 新功能

- 代理模型选择仅影响当前 thread，不隐藏历史会话，不污染后续官方模型请求。
- `thread/list` 显式包含全部 provider，历史列表不再被代理过滤。
- 代理 selection 不写入 Codex 全局 `model`/`model_provider`，仅通过 thread 级临时覆盖生效。
- 启动自愈清理旧版 ProviderDeck 全局选择，让 Codex 回落官方默认配置。
- 零轮次 thread 跳过 unsubscribe/resume 直接发送首个 turn。
- `thread/start` 与 `turn/start` 竞态保护：首 turn 等待 thread 绑定建立后再释放。
- 刷新应用图标和 tray 资源。

### 安全与性能

- 官方请求不经过 ProviderDeck helper。
- 移除全局 direct/proxy 模式和 `provider_sync`。
- 不读取、改写或备份 Codex 历史正文。

## v0.1.0 (2026-07-11)

### 新功能

- 在官方模型列表中加入 scoped proxy models。
- 通过 app-server bridge 在同一任务内重绑定 model/provider。
- 支持 pending switch、验证、rollback 和 recovery journal。
- 提供独立 provider 配置、runtime 状态和诊断管理器。

### 安全与性能

- 官方请求不经过 ProviderDeck helper。
- 移除全局 direct/proxy 模式和 `provider_sync`。
- 不读取、改写或备份 Codex 历史正文。
