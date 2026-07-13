# 模型切换行为

新任务直接使用所选模型和 provider。已有任务在发送下一轮前执行：

```text
thread/unsubscribe -> thread/resume(target) -> verify -> turn/start
```

验证要求 threadId、model 和 modelProvider 同时匹配。运行中的任务只记录最后一次 pending selection；当前 turn 使用原 provider，完成或 interrupt 进入 idle 后再切换。

官方模型固定使用 `modelProvider: "openai"`。代理模型的 selection 形如 `providerdeck:<provider-id>:<model>`，真实模型名可以继续包含冒号。

Codex 临时配置使用不含 `.` 的内部 runtime provider ID。普通 provider ID 保持 `providerdeck-<provider-id>`；包含 `.` 或占用编码保留前缀的 ID 会采用稳定十六进制编码，避免被 Codex 的 dotted keyPath 误解析为嵌套配置。

虚拟 selection 仅用于界面选择，不会写入 Codex 全局 `model` 或 `model_provider`。代理 provider 只通过 `thread/start`、`thread/resume` 和 `turn/start` 的临时配置生效；启动时会清理旧版本遗留的 `providerdeck:*` / `providerdeck-*` 全局选择，让 Codex 回落到官方默认配置，避免历史列表被 provider 过滤或官方模型误走代理。

每个代理 provider 同时提供带本地 bearer 校验的 scoped `GET /provider/<provider-id>/v1/models`，供 Codex 在创建任务前完成模型发现。
