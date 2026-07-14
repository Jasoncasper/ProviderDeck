# 模型切换行为

新任务直接使用所选模型和 provider。已有任务在发送下一轮前执行：

```text
thread/unsubscribe -> thread/resume(target) -> verify -> turn/start
```

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

compaction 失败或超时会阻止用户 turn 发出，并恢复切换前的 provider。检查结果只包含是否需要 compaction 和最后一个不安全 model，不通过 bridge 返回历史正文；compaction 请求只发往该任务已经使用过的原代理 provider。

验证要求 threadId、model 和 modelProvider 同时匹配。运行中的任务只记录最后一次 pending selection；当前 turn 使用原 provider，完成或 interrupt 进入 idle 后再切换。

官方模型固定使用 `modelProvider: "openai"`。代理模型的 selection 形如 `providerdeck:<provider-id>:<model>`，真实模型名可以继续包含冒号。

官方 `gpt-5.3-codex-spark` 当前不接受 `reasoning.summary`，因此其 `turn/start.summary` 会被覆盖为 `none`，由 Codex 在构造 `/responses` 请求时省略该参数；其他模型保留原 summary 设置。

Codex 临时配置使用不含 `.` 的内部 runtime provider ID。普通 provider ID 保持 `providerdeck-<provider-id>`；包含 `.` 或占用编码保留前缀的 ID 会采用稳定十六进制编码，避免被 Codex 的 dotted keyPath 误解析为嵌套配置。

虚拟 selection 仅用于界面选择，不会写入 Codex 全局 `model` 或 `model_provider`。代理 provider 只通过 `thread/start`、`thread/resume` 和 `turn/start` 的临时配置生效；启动时会清理旧版本遗留的 `providerdeck:*` / `providerdeck-*` 全局选择，让 Codex 回落到官方默认配置，避免历史列表被 provider 过滤或官方模型误走代理。

每个代理 provider 同时提供带本地 bearer 校验的 scoped `GET /provider/<provider-id>/v1/models`，供 Codex 在创建任务前完成模型发现。
