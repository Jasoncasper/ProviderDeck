# 模型切换行为

新任务直接使用所选模型和 provider。已有任务在发送下一轮前执行：

```text
thread/unsubscribe -> thread/resume(target) -> verify -> turn/start
```

验证要求 threadId、model 和 modelProvider 同时匹配。运行中的任务只记录最后一次 pending selection；当前 turn 使用原 provider，完成或 interrupt 进入 idle 后再切换。

官方模型固定使用 `modelProvider: "openai"`。代理模型的 selection 形如 `providerdeck:<provider-id>:<model>`，真实模型名可以继续包含冒号。
