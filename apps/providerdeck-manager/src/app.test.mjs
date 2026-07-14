import assert from "node:assert/strict";
import fs from "node:fs";

const source = fs.readFileSync(new URL("./app.tsx", import.meta.url), "utf8");

assert.match(
  source,
  /window\.setInterval\(\(\) => \{\s*void loadRuntime\(\);\s*\}, RUNTIME_STATUS_REFRESH_MS\)/,
  "the home page must poll runtime status after launch",
);
assert.match(
  source,
  /window\.clearInterval\(intervalId\)/,
  "runtime polling must be stopped when the home page is not active",
);

console.log("providerdeck manager runtime polling test passed");
