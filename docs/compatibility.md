# ChatGPT/Codex 兼容矩阵

| ProviderDeck | 平台 | App surface | 状态 |
|---|---|---|---|
| 1.2.4 | macOS 11+ | ChatGPT 26.715.52143 (build 5591) 内置 Codex app-server | 显式重启先完成代理预检，失败时保留 ChatGPT；清理占用 launcher guard 的残留受管进程 |
| 1.2.2 | macOS 11+ | ChatGPT 26.715.52143 (build 5591) 内置 Codex app-server | launcher 正确执行代理就绪等待；watcher 在代理不可用或已有 launcher 接管时保留当前 ChatGPT 进程 |
| 1.2.1 | macOS 11+ | ChatGPT 26.715.52143 (build 5591) 内置 Codex app-server | 启动前等待 VPN/HTTP 代理的 ChatGPT 上游连接就绪，避免远端配置半加载 |
| 1.2.0 | macOS 11+ | ChatGPT 26.715.31925 (build 5551) 内置 Codex app-server | transport 入站/出站窄钩子；保留原生 `model/list` pending lifecycle；内部 IPC 超时收敛；校验代理上游连接 |
| 1.0.18 | macOS 11+ | ChatGPT 26.715.31925 (build 5551) 内置 Codex app-server | 模型 descriptor 对齐 build 5551 完整能力字段 |
| 1.0.17 | macOS 11+ | ChatGPT 26.715.31925 (build 5551) 内置 Codex app-server | 内部切换请求直发 native IPC，避免 renderer scheduler 重入饥饿 |
| 0.1.0 | macOS 11+ | ChatGPT 26.707.41301 (build 5103) 内置 Codex app-server | 本机验证基线 |
| 0.1.0 | macOS 11+ | ChatGPT 26.707.62119 (build 5211) 内置 Codex app-server | 使用 CDP response interception 在 native IPC 前接管请求 |

ChatGPT build 5211 会先调用只读的 `electronBridge.sendMessageFromView`，再派发 `codex-message-from-view` 镜像事件。ProviderDeck 因此在启动时仅 reload renderer 一次，在内存中临时改写 renderer bridge，使 `mcp-request` 在 native IPC 前进入切换逻辑；不会修改 `ChatGPT.app` 文件。transport 健康检查失败时不会重复 reload。兼容条件还包括 `model/list`、`thread/unsubscribe`、`thread/resume` 和 `turn/start` 方法。协议不匹配时 ProviderDeck 应停止启动，避免虚拟模型名进入官方 ChatGPT account 通道。

ChatGPT build 5551 为 AppServer RequestClient 增加了优先级与并发调度。ProviderDeck 拦截中的 `turn/start` 已占用调度槽时，若内部 `thread/read`、`thread/unsubscribe`、`thread/resume` 或 `thread/compact/start` 再进入同一调度器，会形成队列饥饿并连带阻塞模型与历史列表加载。ProviderDeck 1.0.17 改为使用独立 request ID 直发已有 native IPC，在 renderer capture listener 中关联响应，不再重入 ChatGPT RequestClient；普通 ChatGPT 请求仍走原调度链路。

ChatGPT build 5551 的模型对象还新增了 input modalities、personality、speed tier、service tier 与 upgrade 元数据。ProviderDeck 1.0.18 为注入的代理模型补齐同构字段，避免依赖这些字段的模型选择器和任务详情在 renderer 渲染阶段异常。

历史任务详情在恢复前还会通过 `list-models-for-host` 查询当前 model/service tier。ProviderDeck 1.2.0 保留 ChatGPT 原始 `model/list` request ID 与 native pending lifecycle，在 ChatGPT 分发真实响应前补全模型列表；否则 renderer 内部的模型目录 Promise 不会完成，既会隐藏模型选择器，也会让部分历史任务停在 `needs_resume` 且不发出 `thread/read`。同版本还停止改写 `thread/list.modelProviders`，并移除全局 DOM event override，避免干预原生历史、归档与任务恢复状态。ProviderDeck 内部 IPC 在 native 拒绝或超时后会结束原始请求；HTTP 代理启动检查也会验证真实 ChatGPT 上游连接，避免不可用代理拖慢历史加载和消息发送。

ChatGPT build 5591 启动时会拉取远端 Statsig 配置、应用目录和插件状态；当系统只能经本地 VPN 代理访问 ChatGPT，而代理端口已监听但上游节点尚未就绪时，这些请求会长时间超时并导致历史任务与模型选择器进入半加载状态。ProviderDeck 1.2.1 在启动 ChatGPT 前等待代理真实上游连接就绪，连续失败后直接停止启动并提示恢复 VPN 后重试。
