# 模型切换行为

新任务直接使用所选模型和 provider。已有任务在发送下一轮前执行：

```text
thread/unsubscribe -> thread/resume(target) -> verify -> turn/start
```

`thread/start` 响应不一定会回传 renderer，因此对应的首个 `turn/start` 会一次性消费 pending 记录。若用户放弃新任务后进入历史任务，ProviderDeck 通过本机安全检查返回的 `rolloutFound` 区分已有历史与尚未落盘的新任务，避免把历史消息错误地走到新任务首轮路径。

代理模型生成的 reasoning 没有 OpenAI 的 `encrypted_content`，不能作为持久化 item 切回官方 `/responses` 重放。目标为官方模型时，ProviderDeck 先在本机检查当前 rollout；发现不安全 reasoning 后执行：

```text
restore original proxy when needed
  -> thread/compact/start
  -> wait for contextCompaction + turn/completed
  -> thread/unsubscribe
  -> thread/resume(openai)
  -> verify
  -> turn/start
```

compaction 失败或超时会阻止用户 turn 发出，并恢复切换前的 provider。检查结果只包含是否找到 rollout、是否需要 compaction 和最后一个不安全 model，不通过 bridge 返回历史正文；compaction 请求只发往该任务已经使用过的原代理 provider。

验证要求 threadId、model 和 modelProvider 同时匹配。运行中的任务只记录最后一次 pending selection；当前 turn 使用原 provider，完成或 interrupt 进入 idle 后再切换。

ProviderDeck 自己发起的 `thread/read`、`thread/unsubscribe`、`thread/resume` 与 `thread/compact/start` 通过已有 native IPC 独立关联响应，不进入 ChatGPT renderer RequestClient。这样被拦截的 `turn/start` 不会与内部切换请求竞争同一个并发调度槽；模型列表、历史列表和普通 ChatGPT 请求仍使用官方 RequestClient。

官方模型固定使用 `modelProvider: "openai"`。代理模型的 selection 形如 `providerdeck:<provider-id>:<model>`，真实模型名可以继续包含冒号。

官方 `gpt-5.3-codex-spark` 当前不接受 `reasoning.summary`，因此其 `turn/start.summary` 会被覆盖为 `none`，由 Codex 在构造 `/responses` 请求时省略该参数；其他模型保留原 summary 设置。

macOS 的该兼容改写依赖 ProviderDeck 注入当前 ChatGPT renderer。从 ProviderDeck 启动或重启 ChatGPT 后，当前用户级 LaunchAgent watcher 会持续检查运行状态；仅当检测到 ChatGPT 正在运行、CDP 不可达且 launcher guard 不存在时，才关闭该未受管实例并通过 ProviderDeck 重新拉起。关闭 manager 窗口只会隐藏到菜单栏，不卸载 watcher；使用“关闭 ChatGPT 并退出 ProviderDeck”或托盘“退出”会禁用并移除 watcher。

Codex 临时配置使用不含 `.` 的内部 runtime provider ID。普通 provider ID 保持 `providerdeck-<provider-id>`；包含 `.` 或占用编码保留前缀的 ID 会采用稳定十六进制编码，避免被 Codex 的 dotted keyPath 误解析为嵌套配置。

虚拟 selection 仅用于界面选择，不会写入 Codex 全局 `model` 或 `model_provider`。代理 provider 只通过 `thread/start`、`thread/resume` 和 `turn/start` 的临时配置生效；启动时会清理旧版本遗留的 `providerdeck:*` / `providerdeck-*` 全局选择，让 Codex 回落到官方默认配置，避免历史列表被 provider 过滤或官方模型误走代理。

每个代理 provider 同时提供带本地 bearer 校验的 scoped `GET /provider/<provider-id>/v1/models`，供 Codex 在创建任务前完成模型发现。
