# Changelog

## v1.2.5 (2026-07-21)

### 修复

- 放宽启动前代理上游预检的连接超时：`http_proxy_upstream_available` 的 `connect_timeout` 由 1500ms 提升至 6000ms、总超时由 2s 提升至 8s。经 HTTP 代理访问 chatgpt.com 的 TLS 握手实测约 1.5s，原 1500ms 阈值稳定卡在边界，导致代理节点正常时预检仍持续误报"无法建立 ChatGPT 上游连接"而无法重启。

## v1.2.4 (2026-07-21)

### 修复

- 启动前先完成代理网络预检；代理不可用时保留当前 ChatGPT 进程并向管理界面返回明确错误，避免先关闭后启动失败。
- macOS 重启时按 launcher guard 端口清理残留受管 launcher，避免旧进程占用单实例锁导致后续启动被静默忽略。

## v1.2.3 (2026-07-21)

### 修复

- 修复 macOS watcher 启动路径中的竞态导致重复 `bootstrap` 失败，提升多次重启时启动稳定性。

## v1.2.2 (2026-07-21)

### 修复

- 修复静默 launcher 未转发网络就绪检查的问题，确保启动 ChatGPT 前真实等待本地 VPN/HTTP 代理可用。
- watcher 在代理不可用或网络等待期间已有其他 launcher 启动时延后接管，不再终止当前 ChatGPT 进程，避免模型列表刷新子进程被中断并出现消息超时。

## v1.2.1 (2026-07-20)

### 修复

- 启动 ChatGPT 前最多重试约 14 秒，确认本地 VPN/HTTP 代理能够实际建立 ChatGPT 上游连接；代理端口已监听但节点尚未就绪时停止启动并显示明确错误，避免历史任务、模型选择器和远端能力进入半加载状态。

## v1.2.0 (2026-07-20)

### 修复

- `model/list` 保留 ChatGPT 原始 request ID 与 native pending lifecycle，并在 ChatGPT 分发真实响应前合并代理模型，避免模型目录 Promise 悬空后同时阻塞模型选择器与历史任务恢复。
- 不再改写 `thread/list.modelProviders`，历史列表、归档状态与 host/provider lifecycle 完全交由 ChatGPT 原生逻辑管理。
- 移除对全局 `window.dispatchEvent` 的覆盖和全局 `message` capture listener，改为只挂接 AppServer transport 的入站/出站窄钩子，避免干扰新版 ChatGPT 的任务恢复状态。
- bridge 首次安装成功后只轮询 readiness，不再重复创建 CDP session，避免一次启动中多个 session 同时处理同一 binding 回调。
- ProviderDeck 内部 IPC 请求在 native 拒绝或 15 秒无响应时直接结束原始用户请求，避免侧栏任务长期停在“创建任务超时”。
- 本地 HTTP 代理健康检查改为通过该代理实际连接 `https://chatgpt.com`，避免仅端口存活但上游不可用时启动后拖慢历史对话并阻塞消息发送。

## v1.0.18 (2026-07-19)

### 修复

- 补齐 ChatGPT 26.715.31925 (build 5551) 模型 descriptor 新增的能力字段，避免注入的代理模型导致模型选择器和历史任务详情组件在 renderer 中断渲染。

## v1.0.17 (2026-07-19)

### 修复

- 兼容 ChatGPT 26.715.31925 (build 5551) 的 renderer 请求优先级与并发调度：ProviderDeck 内部切换请求改为直发 native IPC 并独立关联响应，避免被拦截的 `turn/start` 重入同一 scheduler 后形成队列饥饿，连带阻塞历史对话与模型选择器。

## v1.0.12 (2026-07-15)

### 修复

- bridge 重注入时保留已有回调，避免历史任务的安全检查悬挂并导致消息提交超时。

## v1.0.11 (2026-07-15)

### 修复

- 切回官方模型前执行 `thread/compact/start` 时忽略非对话的 `additional_tools` 元数据，避免其被错误转换为 `system content:null` 后遭代理上游拒绝；历史压缩不再因连续 `400 Bad Request` 阻塞 `turn/start` 并触发“创建任务超时”。

## v1.0.10 (2026-07-14)

### 修复

- macOS 从 ProviderDeck 启动或重启 ChatGPT 时安装当前用户级 LaunchAgent watcher；ProviderDeck runtime 意外退出后，若用户直接打开缺少 CDP 和 launcher guard 的 ChatGPT，watcher 会一次性接管并通过 ProviderDeck 重新拉起，防止 `gpt-5.3-codex-spark` 再次携带不支持的 `reasoning.summary`。
- 安全退出和托盘退出会禁用并卸载 watcher；安装或启动中途失败会回滚 plist 和禁用状态，避免残留后台自启动项。

## v1.0.9 (2026-07-14)

### 修复

- `thread/start` 响应未回传 renderer 时，首个关联 `turn/start` 会一次性消费 pending 记录，不再让残留状态持续影响后续任务。
- pending 新任务被放弃后，历史任务发送消息会先用本机 `rolloutFound` 判定其已有历史，再执行标准 `thread/read` / `thread/resume`，避免被误判为新任务并提示“创建任务时出错”。

## v1.0.8 (2026-07-14)

### 修复

- 官方 `gpt-5.3-codex-spark` 的 `turn/start` 强制关闭 reasoning summary，避免 Codex 将不受该模型支持的 `reasoning.summary` 发送到 `/responses` 后返回 400。
- 新任务首轮与已有任务继续发送共用同一兼容规则；其他官方模型和代理模型的 summary 设置保持不变。

## v1.0.7 (2026-07-14)

### 修复

- 切回官方模型前检测代理响应中缺少 `encrypted_content` 的 reasoning item，并先在原代理 provider 上执行标准 `thread/compact/start`；等待 compaction 完成后才放行官方 turn，避免官方 `/responses` 重放 `rs_resp_*` 临时 ID 时返回 404。
- 历史任务即使已经错误地绑定到官方模型，也会根据最后一个不安全 reasoning 所属 model 唯一恢复原代理、压缩历史并重新切回官方模型。
- compaction 启动失败、执行失败或超时时不再发送用户 turn，并恢复切换前的 provider 绑定。

### 安全

- 历史安全检查仅在本机读取 rollout，bridge 只返回 `requiresCompaction` 与 model，不返回、记录、改写或备份会话正文；必要的 compaction 只发往该任务已经使用过的原代理 provider。

## v1.0.6 (2026-07-14)

### 修复

- 启动期只缓存 ProviderDeck 必须拦截的 AppServer 请求，让 `account/read` 等认证请求继续走原生 IPC，避免认证状态停在 `no-auth` 后隐藏新任务与历史任务的模型选择器。

## v1.0.5 (2026-07-14)

### 修复

- 修复 AppServer 请求桥插入压缩 renderer bundle 后缺少语句终止符的问题，避免 ChatGPT renderer 语法错误、健康检查失败以及 launcher 自动终止 ChatGPT 进程。

## v1.0.4 (2026-07-14)

### 修复

- 内部 `thread/read`、`thread/unsubscribe`、`thread/resume` 改用 Codex 已注册的 AppServer 请求客户端，不再伪造无法匹配 Promise 的 `providerdeck-*` request ID。
- 新任务尚未物化时，首条 `turn/start` 直接携带目标 provider 发出，避免创建任务失败和 30 秒超时。

## v1.0.3 (2026-07-14)

### 修复

- 在 `thread/start` 尚未完成时，不再对首条消息执行 `thread/read`；避免未落库任务触发 app-server 错误后等待 30 秒超时。

## v1.0.2 (2026-07-14)

### 修复

- 新建任务的首条消息不再等待可能未回传到 renderer 的 `thread/start` 响应，直接携带已选 provider 配置进入原生 IPC 队列，避免 30 秒超时。

## v1.0.1 (2026-07-14)

### 修复

- 代理 Chat Completion 到 Responses 的兼容层为 assistant message 生成合法的 `msg_*` ID，避免历史会话继续时被上游拒绝。

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
