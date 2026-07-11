# Provider 配置

每个代理 provider 需要唯一的 `id`、显示名称、API base URL、API key、协议和目标模型。`id` 只允许字母、数字、点、下划线和连字符，且不能以点开头或结尾。

ProviderDeck 为每个启用的 provider 生成独立 helper URL：

```text
http://127.0.0.1:<port>/provider/<provider-id>/v1
```

上游 API key 只保存在 `~/.providerdeck/routing.toml` 并由 helper 使用。renderer 只收到随机 runtime bearer token，不收到上游 API key。
