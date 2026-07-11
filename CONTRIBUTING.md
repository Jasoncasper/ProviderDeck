# Contributing to ProviderDeck

感谢你关注 ProviderDeck。

## 开始之前

- 提交 issue 前，请先确认问题可以稳定复现，或需求边界足够清晰
- 提交 PR 前，请确保修改范围尽量聚焦，不顺手重构无关模块
- 如果改动会影响打包、更新、注入或路由逻辑，请在 PR 描述里明确说明

## 开发环境

- macOS 12+
- Rust stable
- Node.js 20+
- npm 10+

## 本地启动

```bash
cd apps/providerdeck-manager
npm install
npm run dev
```

## 提交前检查

```bash
cd apps/providerdeck-manager
npm run check

cd /path/to/ProviderDeck
cargo check --workspace
cargo test -p providerdeck-manager --lib
```

如果你改动了注入、路由或更新相关逻辑，建议额外运行：

```bash
cargo test -p providerdeck-core --test cdp_bridge
```

## 分支与提交建议

- 分支命名：`feat/...`、`fix/...`、`docs/...`
- 提交信息建议使用简短前缀：`feat:`、`fix:`、`docs:`、`refactor:`

## Pull Request 说明

PR 描述至少应包含：

- 修改目的
- 主要改动点
- 验证方式
- 是否影响安装包、更新、注入或兼容性

## 行为准则

- 保持讨论聚焦技术问题
- 尊重现有设计边界
- 对外部接口改动要明确说明兼容性影响
