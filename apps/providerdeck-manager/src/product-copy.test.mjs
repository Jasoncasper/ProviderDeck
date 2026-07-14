import assert from "node:assert/strict";
import fs from "node:fs";

const cases = [
  ["./components/launch/LaunchPage.tsx", "ChatGPT/Codex 连接与 provider 切换状态", "ChatGPT 连接与 provider 切换状态"],
  ["./components/about/UpdateCheck.tsx", "Codex 版本", "ChatGPT 版本"],
  ["./components/about/HealthStatus.tsx", "Codex 应用路径", "ChatGPT 应用路径"],
  ["../src-tauri/src/commands.rs", "ChatGPT/Codex 尚未完全退出", "ChatGPT 尚未完全退出"],
  ["../src-tauri/src/commands.rs", "Codex 已请求重启", "ChatGPT 已请求重启"],
];

for (const [path, outdated, current] of cases) {
  const source = fs.readFileSync(new URL(path, import.meta.url), "utf8");
  assert.doesNotMatch(source, new RegExp(outdated), `${path} must not display the old Codex product name`);
  assert.match(source, new RegExp(current), `${path} must display ChatGPT consistently`);
}

console.log("providerdeck product copy tests passed");
