# ChatGPT/Codex 兼容矩阵

| ProviderDeck | 平台 | App surface | 状态 |
|---|---|---|---|
| 0.1.0 | macOS 11+ | ChatGPT 26.707.41301 (build 5103) 内置 Codex app-server | 本机验证基线 |

兼容条件包括 `codex-message-from-view` / `mcp-request` bridge，以及 `model/list`、`thread/unsubscribe`、`thread/resume` 和 `turn/start` 方法。协议不匹配时 ProviderDeck 不应接管官方请求，并提示退出 ProviderDeck 或使用 CodexMate。
