# Changelog

## v0.1.0 (2026-07-11)

### 新功能

- 在官方模型列表中加入 scoped proxy models。
- 通过 app-server bridge 在同一任务内重绑定 model/provider。
- 支持 pending switch、验证、rollback 和 recovery journal。
- 提供独立 provider 配置、runtime 状态和诊断管理器。
- 提供只读 CodexMate provider 配置导入。

### 安全与性能

- 官方请求不经过 ProviderDeck helper。
- 移除全局 direct/proxy 模式和 `provider_sync`。
- 不读取、改写或备份 Codex 历史正文。
