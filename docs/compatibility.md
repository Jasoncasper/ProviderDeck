# ChatGPT/Codex 兼容矩阵

| ProviderDeck | 平台 | App surface | 状态 |
|---|---|---|---|
| 1.0.17 | macOS 11+ | ChatGPT 26.715.31925 (build 5551) 内置 Codex app-server | 内部切换请求直发 native IPC，避免 renderer scheduler 重入饥饿 |
| 0.1.0 | macOS 11+ | ChatGPT 26.707.41301 (build 5103) 内置 Codex app-server | 本机验证基线 |
| 0.1.0 | macOS 11+ | ChatGPT 26.707.62119 (build 5211) 内置 Codex app-server | 使用 CDP response interception 在 native IPC 前接管请求 |

ChatGPT build 5211 会先调用只读的 `electronBridge.sendMessageFromView`，再派发 `codex-message-from-view` 镜像事件。ProviderDeck 因此在启动时仅 reload renderer 一次，在内存中临时改写 renderer bridge，使 `mcp-request` 在 native IPC 前进入切换逻辑；不会修改 `ChatGPT.app` 文件。transport 健康检查失败时不会重复 reload。兼容条件还包括 `model/list`、`thread/unsubscribe`、`thread/resume` 和 `turn/start` 方法。协议不匹配时 ProviderDeck 应停止启动，避免虚拟模型名进入官方 ChatGPT account 通道。

ChatGPT build 5551 为 AppServer RequestClient 增加了优先级与并发调度。ProviderDeck 拦截中的 `turn/start` 已占用调度槽时，若内部 `thread/read`、`thread/unsubscribe`、`thread/resume` 或 `thread/compact/start` 再进入同一调度器，会形成队列饥饿并连带阻塞模型与历史列表加载。ProviderDeck 1.0.17 改为使用独立 request ID 直发已有 native IPC，在 renderer capture listener 中关联响应，不再重入 ChatGPT RequestClient；普通 ChatGPT 请求仍走原调度链路。
