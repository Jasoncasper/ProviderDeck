# 故障恢复

目标 provider 切换失败时，ProviderDeck 会重新 unsubscribe 并 resume 原 model/provider。恢复成功后任务保持可用，失败记录保存在 `~/.providerdeck/switch-journal.json`，状态为 `recovery_required`。

管理器的“重新注入并恢复 runtime”会重建 CDP bridge 并清理已恢复的 journal。仍无法恢复时，安全退出 ProviderDeck，再独立启动 CodexMate v1.0.9。两个软件不共享进程、状态目录或更新源。

Journal 不包含 API key 或 bearer token，也不会写入 Codex 历史目录。
