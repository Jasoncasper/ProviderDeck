# 与 CodexMate 共存

ProviderDeck 使用 `ProviderDeck.app`、`com.jasoncasper.providerdeck`、`~/.providerdeck` 和独立 helper 端口。它不会覆盖 `/Applications/CodexMate.app` 或 `~/.codex-session-delete`。

不要让两个软件同时控制同一个 ChatGPT 实例。ProviderDeck 启动时会通过 CDP/guard 端口检测同类控制进程；退出时停止自己的 helper、watchdog 和 CDP 状态。
