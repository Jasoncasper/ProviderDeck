import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

const script = fs.readFileSync(new URL("./renderer-inject.js", import.meta.url), "utf8");

const officialModel = {
  model: "gpt-5.4",
  id: "gpt-5.4",
  displayName: "GPT-5.4",
  description: "Official",
  hidden: false,
  isDefault: true,
  defaultReasoningEffort: "medium",
  supportedReasoningEfforts: [],
};

const catalog = {
  status: "ok",
  models: [
    {
      selection: "providerdeck:team_proxy:vendor:model:v2",
      model: "vendor:model:v2",
      providerId: "team_proxy",
      source: "proxy",
      displayName: "Vendor V2",
      description: "Team Proxy",
      supportedReasoningEfforts: ["low", "medium", "high"],
    },
  ],
  providers: {
    team_proxy: {
      runtimeProviderId: "providerdeck-team_proxy",
      name: "Team Proxy",
      baseUrl: "http://127.0.0.1:57322/provider/team_proxy/v1",
      bearerToken: "local-runtime-token",
    },
  },
};

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
async function drain() {
  for (let index = 0; index < 8; index += 1) await tick();
}

async function createHarness(rpcHandler = async (method, params) => {
  if (method === "model/list") return { data: [structuredClone(officialModel)] };
  if (method === "thread/unsubscribe") return { status: "unsubscribed" };
  if (method === "thread/read") return { thread: { id: params.threadId, model: "gpt-5.4", modelProvider: "openai", status: { type: "idle" } } };
  if (method === "thread/resume") {
    return {
      thread: { id: params.threadId, status: { type: "idle" } },
      model: params.model,
      modelProvider: params.modelProvider,
    };
  }
  if (method === "thread/start") {
    return {
      thread: { id: "thread-new", status: { type: "idle" } },
      model: params.model,
      modelProvider: params.modelProvider,
    };
  }
  return { turn: { id: "turn-1", status: "inProgress" } };
}) {
  const listeners = new Map();
  const nativeRequests = [];
  const bridgeCalls = [];
  const emittedMessages = [];
  const fetchCalls = [];

  class CustomEvent {
    constructor(type, init = {}) { this.type = type; this.detail = init.detail; }
  }
  class MessageEvent {
    constructor(type, init = {}) { this.type = type; this.data = init.data; }
  }
  const emitMessage = (data) => {
    emittedMessages.push(data);
    for (const listener of listeners.get("message") ?? []) listener({ type: "message", data });
  };
  const originalDispatch = (event) => {
    if (event.type === "message") {
      emitMessage(event.data);
      return true;
    }
    if (event.type !== "codex-message-from-view" || event.detail?.type !== "mcp-request") return true;
    const request = structuredClone(event.detail.request);
    nativeRequests.push(request);
    Promise.resolve()
      .then(() => rpcHandler(request.method, request.params ?? {}))
      .then((result) => emitMessage({ type: "mcp-response", message: { id: request.id, result } }))
      .catch((error) => emitMessage({ type: "mcp-response", message: { id: request.id, error: { message: error.message } } }));
    return true;
  };
  const sandbox = {
    CustomEvent,
    MessageEvent,
    URL,
    console,
    globalThis: null,
    window: null,
    document: { body: null, head: null, documentElement: null },
    localStorage: { getItem: () => null, setItem() {} },
    setTimeout,
    clearTimeout,
    setInterval() {},
    requestAnimationFrame: (callback) => callback(),
    addEventListener(type, listener) {
      if (!listeners.has(type)) listeners.set(type, []);
      listeners.get(type).push(listener);
    },
    dispatchEvent: originalDispatch,
    fetch(input, init) {
      fetchCalls.push({ input, init });
      return Promise.resolve({ ok: true });
    },
  };
  sandbox.window = sandbox;
  sandbox.globalThis = sandbox;
  sandbox.__providerDeckBridge = async (path, payload) => {
    bridgeCalls.push({ path, payload: structuredClone(payload ?? {}) });
    if (path === "/providerdeck/catalog") return structuredClone(catalog);
    return { status: "ok" };
  };
  vm.runInNewContext(script, sandbox, { filename: "renderer-inject.js" });
  await drain();
  return { sandbox, nativeRequests, bridgeCalls, emittedMessages, fetchCalls, emitMessage };
}

function requestEvent(harness, id, method, params) {
  harness.sandbox.dispatchEvent(new harness.sandbox.CustomEvent("codex-message-from-view", {
    detail: { type: "mcp-request", hostId: "local", request: { id, method, params } },
  }));
}

async function establishBinding(harness, model, modelProvider) {
  requestEvent(harness, 10, "thread/resume", { threadId: "thread-1", model, modelProvider });
  await drain();
}

{
  const harness = await createHarness();
  requestEvent(harness, 1, "model/list", { includeHidden: false });
  await drain();
  const response = harness.emittedMessages.find((message) => message.message?.id === 1);
  const models = response.message.result.data;
  assert.equal(models[0].model, "gpt-5.4", "official models must stay unchanged");
  assert.equal(models[1].model, "providerdeck:team_proxy:vendor:model:v2");
  assert.equal(models[1].displayName, "Vendor V2");
  assert.equal(harness.nativeRequests[0].params.includeHidden, true);
}

{
  const harness = await createHarness();
  requestEvent(harness, 31, "turn/start", {
    threadId: "thread-not-yet-tracked",
    model: "providerdeck:team_proxy:vendor:model:v2",
    input: [],
  });
  await drain();
  const methods = harness.nativeRequests.map((request) => request.method);
  assert.deepEqual(methods, ["thread/read", "thread/unsubscribe", "thread/resume", "turn/start"]);
  assert.equal(harness.nativeRequests[0].params.includeTurns, false);
}

{
  const historyPayload = { data: [{ id: "thread-1", messages: [{ model: "historical-model" }] }] };
  const harness = await createHarness(async (method) => method === "thread/list" ? structuredClone(historyPayload) : {});
  requestEvent(harness, 2, "thread/list", {});
  await drain();
  const response = harness.emittedMessages.find((message) => message.message?.id === 2);
  assert.deepEqual(response.message.result, historyPayload, "history payloads must remain byte-shape equivalent");
}

{
  const harness = await createHarness();
  await harness.sandbox.fetch("https://chatgpt.com/backend-api/responses", {
    body: JSON.stringify({ model: "providerdeck:team_proxy:vendor:model:v2" }),
  });
  assert.equal(harness.fetchCalls[0].input, "https://chatgpt.com/backend-api/responses");
}

{
  const harness = await createHarness();
  requestEvent(harness, 3, "thread/start", {
    model: "providerdeck:team_proxy:vendor:model:v2",
    cwd: "/tmp/project",
  });
  await drain();
  const start = harness.nativeRequests.find((request) => request.id === 3);
  assert.equal(start.params.model, "vendor:model:v2");
  assert.equal(start.params.modelProvider, "providerdeck-team_proxy");
  assert.equal(
    start.params.config["model_providers.providerdeck-team_proxy"].base_url,
    "http://127.0.0.1:57322/provider/team_proxy/v1",
  );
}

{
  const harness = await createHarness();
  await establishBinding(harness, "gpt-5.4", "openai");
  requestEvent(harness, 4, "turn/start", {
    threadId: "thread-1",
    model: "providerdeck:team_proxy:vendor:model:v2",
    input: [{ type: "text", text: "hello" }],
  });
  await drain();
  const methods = harness.nativeRequests.slice(1).map((request) => request.method);
  assert.deepEqual(methods, ["thread/unsubscribe", "thread/resume", "turn/start"]);
  const turn = harness.nativeRequests.at(-1);
  assert.equal(turn.params.model, "vendor:model:v2");
}

{
  const harness = await createHarness();
  await establishBinding(harness, "gpt-5.4", "openai");
  harness.emitMessage({
    type: "mcp-notification",
    message: { method: "thread/status/changed", params: { threadId: "thread-1", status: { type: "active" } } },
  });
  requestEvent(harness, 5, "turn/start", {
    threadId: "thread-1",
    model: "providerdeck:team_proxy:vendor:model:v2",
    input: [{ type: "text", text: "queued" }],
  });
  await drain();
  assert.equal(harness.nativeRequests.some((request) => request.id === 5), false);
  assert.equal(harness.bridgeCalls.at(-1).payload.phase, "pending");

  harness.emitMessage({
    type: "mcp-notification",
    message: { method: "turn/completed", params: { threadId: "thread-1", turn: { status: "completed" } } },
  });
  await drain();
  assert.equal(harness.nativeRequests.at(-1).id, 5);
}

{
  const harness = await createHarness();
  await establishBinding(harness, "gpt-5.4", "openai");
  harness.emitMessage({
    type: "mcp-notification",
    message: { method: "thread/status/changed", params: { threadId: "thread-1", status: { type: "active" } } },
  });
  requestEvent(harness, 51, "turn/start", { threadId: "thread-1", model: "gpt-5.4", input: [] });
  requestEvent(harness, 52, "turn/start", { threadId: "thread-1", model: "providerdeck:team_proxy:vendor:model:v2", input: [] });
  await drain();
  const superseded = harness.emittedMessages.find((message) => message.message?.id === 51);
  assert.match(superseded.message.error.message, /superseded/);

  harness.emitMessage({
    type: "mcp-notification",
    message: { method: "thread/status/changed", params: { threadId: "thread-1", status: { type: "idle" } } },
  });
  await drain();
  assert.equal(harness.nativeRequests.at(-1).id, 52, "idle after interrupt must release only the latest turn");
}

{
  const harness = await createHarness();
  requestEvent(harness, 53, "thread/start", { model: "gpt-5.4" });
  await drain();
  const start = harness.nativeRequests.find((request) => request.id === 53);
  assert.equal(start.params.modelProvider, "openai");
  assert.equal(start.params.config, undefined, "official models must not receive helper config");
}

{
  const harness = await createHarness(async (method, params) => {
    if (method === "thread/unsubscribe") return { status: "unsubscribed" };
    if (method === "thread/resume" && params.modelProvider === "providerdeck-team_proxy") {
      throw new Error("target unavailable");
    }
    if (method === "thread/resume") {
      return { thread: { id: params.threadId, status: { type: "idle" } }, model: params.model, modelProvider: params.modelProvider };
    }
    return {};
  });
  await establishBinding(harness, "gpt-5.4", "openai");
  requestEvent(harness, 6, "turn/start", {
    threadId: "thread-1",
    model: "providerdeck:team_proxy:vendor:model:v2",
    input: [{ type: "text", text: "rollback" }],
  });
  await drain();
  const resumes = harness.nativeRequests.filter((request) => request.method === "thread/resume");
  assert.equal(resumes.at(-1).params.modelProvider, "openai", "failed target must restore original provider");
  assert.equal(harness.nativeRequests.some((request) => request.id === 6), false);
  assert.equal(harness.bridgeCalls.some((call) => call.payload.phase === "rolling_back"), true);
}

console.log("providerdeck renderer injection tests passed");
