import assert from "node:assert/strict";
import fs from "node:fs";

const source = fs.readFileSync(new URL("./LaunchPage.tsx", import.meta.url), "utf8");

assert.doesNotMatch(
  source,
  /journal\.record\.(threadId|target)/,
  "internal task and target metadata must not be rendered on the runtime page",
);

assert.match(
  source,
  /const primaryAction = needsRecovery \? onRecover : onRestart;/,
  "the primary action must recover only when runtime recovery is required",
);

assert.match(
  source,
  /needsRecovery \? "重启并恢复 runtime" : runtime\?\.appServerConnected \? "重启 ChatGPT" : "启动 ChatGPT"/,
  "the primary button label must describe the action for the current runtime state",
);

assert.doesNotMatch(
  source,
  /重新注入并恢复 runtime/,
  "the duplicate standalone recovery button must not be rendered",
);

assert.match(
  source,
  /关闭 ChatGPT 并退出 ProviderDeck/,
  "the exit button must state that it also closes ChatGPT",
);

assert.doesNotMatch(
  source,
  />\{phase\}<\/Badge>/,
  "internal switch phase values must not be rendered directly",
);

assert.doesNotMatch(source, /SWITCH_STATUSES|模型切换|switchStatus/, "the runtime page must not render a model switch status card");
assert.match(source, /grid gap-3 sm:grid-cols-2 lg:grid-cols-3/, "the remaining runtime cards must use a three-column desktop grid");

console.log("providerdeck launch page tests passed");
