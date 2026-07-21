# ProviderDeck

ProviderDeck 在 macOS ChatGPT/Codex 的原生模型选择器中合并官方模型和 OpenAI-compatible 代理模型，并在同一任务的轮次边界安全切换 provider。

## 设计边界

- 官方模型保持 `modelProvider: "openai"`，请求不经过 ProviderDeck helper。
- 代理模型使用 `providerdeck:<provider-id>:<model>` selection，仅代理请求进入本地 helper。
- 已有任务通过 `thread/unsubscribe` 和 `thread/resume` 重绑定，不改写 threadId、名称或历史消息。
- 运行中的 turn 继续使用原 provider，切换在完成或 interrupt 后进入 idle 时生效。
- 切换失败自动恢复原 provider；恢复失败进入 `recovery_required`。
- 切回官方模型前仅在本机只读检查 Codex rollout 的 reasoning 元数据；不向 renderer 或日志返回正文，不改写或备份历史，必要的 compaction 只发往该任务已经使用过的原代理 provider。
- macOS 从 ProviderDeck 启动 ChatGPT 后会安装当前用户级 runtime watcher；若之后直接打开未受管的 ChatGPT，watcher 会将其重启为 ProviderDeck 受管实例。安全退出 ProviderDeck 时会移除 watcher。
- Plugins、Skills 和 MCP 继续由官方 ChatGPT/Codex 管理。


## 本地开发

```bash
node assets/inject/renderer-inject.test.mjs
cargo test --workspace
npm ci --prefix apps/providerdeck-manager
npm run check --prefix apps/providerdeck-manager
npm run vite:build --prefix apps/providerdeck-manager
```

macOS 构建：

```bash
cargo build --release --workspace
bash scripts/installer/macos/package-dmg.sh 1.2.2 aarch64
```

## 配置

ProviderDeck 状态保存在 `~/.providerdeck/`：

```text
settings.json
routing.toml
switch-journal.json
logs/
updates/
```


## 兼容性

首版以 macOS ChatGPT 当前内置的 app-server MCP bridge 为目标。ProviderDeck 检测到协议不兼容时应停止统一切换并保留官方直连能力。兼容记录见 [版本矩阵](docs/compatibility.md)。

## License

All Rights Reserved。代码公开但不授予使用、修改或分发权利。
