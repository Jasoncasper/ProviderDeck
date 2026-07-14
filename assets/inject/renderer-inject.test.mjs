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
}, transportLoaded = true, pendingPostMessages = [], runtimeCatalog = catalog, options = {}) {
  const listeners = new Map();
  const nativeRequests = [];
  const appCommandCalls = [];
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
  const sendNative = (detail) => {
    const request = structuredClone(detail.request);
    nativeRequests.push(request);
    if (options.orphanProviderDeckResponses && String(request.id).startsWith("providerdeck-")) {
      void Promise.resolve().then(() => rpcHandler(request.method, request.params ?? {}));
      return Promise.resolve();
    }
    Promise.resolve()
      .then(() => rpcHandler(request.method, request.params ?? {}))
      .then((result) => emitMessage({ type: "mcp-response", message: { id: request.id, result } }))
      .catch((error) => emitMessage({ type: "mcp-response", message: { id: request.id, error: { message: error.message } } }));
    return Promise.resolve();
  };
  const originalDispatch = (event) => {
    if (event.type === "message") {
      emitMessage(event.data);
      return true;
    }
    if (event.type !== "codex-message-from-view" || event.detail?.type !== "mcp-request") return true;
    void sendNative(event.detail);
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
    electronBridge: { sendMessageFromView: sendNative },
    __providerDeckTransportPatchLoaded: transportLoaded,
    __providerDeckPendingPostMessages: structuredClone(pendingPostMessages),
    fetch(input, init) {
      fetchCalls.push({ input, init });
      return Promise.resolve({ ok: true });
    },
  };
  sandbox.window = sandbox;
  sandbox.globalThis = sandbox;
  sandbox.__providerDeckSendCliRequest = async (payload) => {
    const call = structuredClone(payload);
    appCommandCalls.push(call);
    nativeRequests.push({
      id: `app-command-${appCommandCalls.length}`,
      method: call.method,
      params: call.params ?? {},
    });
    return rpcHandler(call.method, call.params ?? {});
  };
  sandbox.__providerDeckBridge = async (path, payload) => {
    bridgeCalls.push({ path, payload: structuredClone(payload ?? {}) });
    if (path === "/providerdeck/catalog") return structuredClone(runtimeCatalog);
    return { status: "ok" };
  };
  vm.runInNewContext(script, sandbox, { filename: "renderer-inject.js" });
  await drain();
  return { sandbox, nativeRequests, appCommandCalls, bridgeCalls, emittedMessages, fetchCalls, emitMessage };
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
  const harness = await createHarness(undefined, true, [], catalog, {
    orphanProviderDeckResponses: true,
  });
  requestEvent(harness, 31, "turn/start", {
    threadId: "thread-not-yet-tracked",
    model: "providerdeck:team_proxy:vendor:model:v2",
    input: [],
  });
  await drain();
  const methods = harness.nativeRequests.map((request) => request.method);
  assert.deepEqual(methods, ["thread/read", "thread/unsubscribe", "thread/resume", "turn/start"]);
  assert.deepEqual(
    harness.appCommandCalls.map((call) => call.method),
    ["thread/read", "thread/unsubscribe", "thread/resume"],
    "internal RPC must use Codex's registered request client",
  );
  assert.equal(harness.nativeRequests[0].params.includeTurns, true);
}

{
  const harness = await createHarness(async (method) => {
    if (method === "thread/read") {
      throw new Error("thread thread-new is not materialized yet; includeTurns is unavailable before first user message");
    }
    return { turn: { id: "turn-first", status: "inProgress" } };
  }, true, [], catalog, { orphanProviderDeckResponses: true });
  requestEvent(harness, 38, "turn/start", {
    threadId: "thread-new",
    model: "providerdeck:team_proxy:vendor:model:v2",
    input: [{ type: "text", text: "first turn" }],
  });
  await drain();
  assert.deepEqual(
    harness.nativeRequests.map((request) => request.method),
    ["thread/read", "turn/start"],
    "an unmaterialized thread must receive its first turn without resume",
  );
  assert.deepEqual(harness.appCommandCalls.map((call) => call.method), ["thread/read"]);
}

{
  const harness = await createHarness(async (method, params) => {
    if (method === "thread/read") {
      return {
        thread: {
          id: params.threadId,
          modelProvider: "openai",
          status: { type: "idle" },
          turns: [],
        },
      };
    }
    if (method === "thread/unsubscribe") return { status: "unsubscribed" };
    if (method === "thread/resume") {
      return {
        thread: { id: params.threadId, status: { type: "idle" } },
        model: params.model,
        modelProvider: params.modelProvider,
      };
    }
    return { turn: { id: "turn-first", status: "inProgress" } };
  });
  requestEvent(harness, 35, "turn/start", {
    threadId: "thread-created-outside-renderer-hook",
    model: "providerdeck:team_proxy:vendor:model:v2",
    input: [{ type: "text", text: "first turn" }],
  });
  await drain();
  assert.deepEqual(
    harness.nativeRequests.map((request) => request.method),
    ["thread/read", "turn/start"],
    "a zero-turn thread must send its first turn without unsubscribe/resume",
  );
  assert.equal(harness.nativeRequests[0].params.includeTurns, true);
  assert.equal(harness.nativeRequests[1].params.model, "vendor:model:v2");
  assert.equal(harness.nativeRequests[1].params.modelProvider, "providerdeck-team_proxy");
}

{
  const historyPayload = { data: [{ id: "thread-1", messages: [{ model: "historical-model" }] }] };
  const harness = await createHarness(async (method) => method === "thread/list" ? structuredClone(historyPayload) : {});
  requestEvent(harness, 2, "thread/list", { modelProviders: ["providerdeck-team_proxy"] });
  await drain();
  const response = harness.emittedMessages.find((message) => message.message?.id === 2);
  assert.deepEqual(response.message.result, historyPayload, "history payloads must remain byte-shape equivalent");
  assert.deepEqual(harness.nativeRequests[0].params.modelProviders, [], "history must include every provider");
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
    collaborationMode: {
      mode: "default",
      settings: { model: "providerdeck:team_proxy:vendor:model:v2", reasoningEffort: "high" },
    },
    cwd: "/tmp/project",
  });
  await drain();
  const start = harness.nativeRequests.find((request) => request.id === 3);
  assert.equal(start.params.model, "vendor:model:v2");
  assert.equal(start.params.modelProvider, "providerdeck-team_proxy");
  assert.equal(start.params.collaborationMode.settings.model, "vendor:model:v2");
  assert.equal(
    start.params.config["model_providers.providerdeck-team_proxy"].base_url,
    "http://127.0.0.1:57322/provider/team_proxy/v1",
  );
}

{
  const harness = await createHarness(async (method, params) => {
    if (method === "thread/start") {
      return { thread: { id: "thread-fresh", status: { type: "idle" } } };
    }
    if (method === "thread/read") {
      return { thread: { id: params.threadId, status: { type: "idle" } } };
    }
    if (method === "thread/unsubscribe") return { status: "unsubscribed" };
    if (method === "thread/resume") {
      return {
        thread: { id: params.threadId, status: { type: "idle" } },
        model: params.model,
        modelProvider: params.modelProvider,
      };
    }
    return { turn: { id: "turn-fresh", status: "inProgress" } };
  });
  requestEvent(harness, 33, "thread/start", { model: "gpt-5.4" });
  await drain();
  requestEvent(harness, 34, "turn/start", {
    threadId: "thread-fresh",
    model: "gpt-5.4",
    input: [{ type: "text", text: "first turn" }],
  });
  await drain();
  assert.deepEqual(
    harness.nativeRequests.map((request) => request.method),
    ["thread/start", "turn/start"],
    "a fresh thread must not be resumed before its first turn is persisted",
  );
}

{
  const harness = await createHarness(async (method) => {
    if (method === "thread/start") return new Promise(() => {});
    if (method === "thread/read") return new Promise(() => {});
    return { turn: { id: "turn-racing", status: "inProgress" } };
  });
  requestEvent(harness, 36, "thread/start", {
    model: "gpt-5.4",
  });
  await tick();
  requestEvent(harness, 37, "turn/start", {
    threadId: "thread-racing",
    model: "providerdeck:team_proxy:vendor:model:v2",
    input: [{ type: "text", text: "first turn races thread start" }],
  });
  await drain();
  assert.deepEqual(
    harness.nativeRequests.map((request) => request.method),
    ["thread/start", "turn/start"],
    "the first turn must not read a thread that is still being created",
  );
  const turn = harness.nativeRequests.at(-1);
  assert.equal(turn.params.model, "vendor:model:v2");
  assert.equal(turn.params.modelProvider, "providerdeck-team_proxy");
}

{
  const harness = await createHarness();
  requestEvent(harness, 32, "config/value/write", {
    keyPath: "model",
    value: "providerdeck:team_proxy:vendor:model:v2",
    mergeStrategy: "replace",
    expectedVersion: "config-v1",
  });
  await drain();
  const write = harness.nativeRequests.find((request) => request.id === 32);
  assert.equal(write.method, "config/batchWrite");
  assert.deepEqual(JSON.parse(JSON.stringify(write.params.edits)), [
    {
      keyPath: "model_providers.providerdeck-team_proxy",
      value: {
        name: "Team Proxy",
        base_url: "http://127.0.0.1:57322/provider/team_proxy/v1",
        wire_api: "responses",
        requires_openai_auth: false,
        env_key: "PROVIDERDECK_RUNTIME_TOKEN",
      },
      mergeStrategy: "replace",
    },
  ]);
  assert.equal(write.params.expectedVersion, "config-v1");
  assert.equal(write.params.reloadUserConfig, true);
}

{
  const harness = await createHarness();
  requestEvent(harness, 33, "config/batchWrite", {
    edits: [
      { keyPath: "model", value: "providerdeck:team_proxy:vendor:model:v2", mergeStrategy: "replace" },
      { keyPath: "model_reasoning_effort", value: "high", mergeStrategy: "replace" },
    ],
    expectedVersion: "config-v2",
  });
  await drain();
  const write = harness.nativeRequests.find((request) => request.id === 33);
  assert.equal(write.params.edits[0].keyPath, "model_reasoning_effort");
  assert.equal(write.params.edits[1].keyPath, "model_providers.providerdeck-team_proxy");
  assert.equal(write.params.edits.some((edit) => edit.keyPath === "model"), false);
  assert.equal(write.params.edits.some((edit) => edit.keyPath === "model_provider"), false);
  assert.equal(write.params.reloadUserConfig, true);
}

{
  const harness = await createHarness();
  await establishBinding(harness, "gpt-5.4", "openai");
  requestEvent(harness, 4, "turn/start", {
    threadId: "thread-1",
    collaborationMode: {
      mode: "default",
      settings: {
        model: "providerdeck:team_proxy:vendor:model:v2",
        reasoningEffort: "high",
      },
    },
    input: [{ type: "text", text: "hello" }],
  });
  await drain();
  const methods = harness.nativeRequests.slice(1).map((request) => request.method);
  assert.deepEqual(methods, ["thread/unsubscribe", "thread/resume", "turn/start"]);
  const turn = harness.nativeRequests.at(-1);
  assert.equal(turn.params.model, "vendor:model:v2");
  assert.equal(turn.params.modelProvider, "providerdeck-team_proxy");
  assert.equal(turn.params.collaborationMode.settings.model, "vendor:model:v2");
  assert.equal(turn.params.collaborationMode.settings.reasoningEffort, "high");
  assert.equal(
    turn.params.config["model_providers.providerdeck-team_proxy"].base_url,
    "http://127.0.0.1:57322/provider/team_proxy/v1",
  );
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

{
  const dottedCatalog = structuredClone(catalog);
  dottedCatalog.providers["glm-5.2"] = {
    runtimeProviderId: "providerdeck-pdhex-676c6d2d352e32",
    name: "GLM 5.2",
    baseUrl: "http://127.0.0.1:57322/provider/glm-5.2/v1",
    bearerToken: "local-runtime-token",
  };
  const harness = await createHarness(async (method, params) => {
    if (method === "thread/unsubscribe") return { status: "unsubscribed" };
    if (method === "thread/resume" && params.modelProvider === "providerdeck-team_proxy") {
      throw new Error("target unavailable");
    }
    if (method === "thread/resume") {
      return { thread: { id: params.threadId, status: { type: "idle" } }, model: params.model, modelProvider: params.modelProvider };
    }
    return {};
  }, true, [], dottedCatalog);
  await establishBinding(harness, "glm-5.2", "providerdeck-pdhex-676c6d2d352e32");
  requestEvent(harness, 64, "turn/start", {
    threadId: "thread-1",
    model: "providerdeck:team_proxy:vendor:model:v2",
    input: [],
  });
  await drain();
  const resumes = harness.nativeRequests.filter((request) => request.method === "thread/resume");
  const rollback = resumes.at(-1);
  assert.equal(rollback.params.modelProvider, "providerdeck-pdhex-676c6d2d352e32");
  assert.equal(
    rollback.params.config["model_providers.providerdeck-pdhex-676c6d2d352e32"].base_url,
    "http://127.0.0.1:57322/provider/glm-5.2/v1",
  );
}

{
  const harness = await createHarness();
  assert.equal(
    typeof harness.sandbox.__providerDeckInterceptPostMessage,
    "function",
    "the authoritative renderer bridge must expose a pre-IPC hook",
  );
  const detail = {
    type: "mcp-request",
    hostId: "local",
    request: {
      id: 61,
      method: "turn/start",
      params: {
        threadId: "thread-authoritative",
        model: "providerdeck:team_proxy:vendor:model:v2",
        collaborationMode: {
          mode: "default",
          settings: { model: "providerdeck:team_proxy:vendor:model:v2" },
        },
      },
    },
  };
  assert.equal(harness.sandbox.__providerDeckInterceptPostMessage(detail), true);
  await drain();
  const turn = harness.nativeRequests.find((request) => request.id === 61);
  assert.ok(turn, "the deferred turn must eventually reach native IPC");
  assert.equal(turn.params.model, "vendor:model:v2");
  assert.equal(turn.params.modelProvider, "providerdeck-team_proxy");
  assert.equal(turn.params.collaborationMode.settings.model, "vendor:model:v2");
}

{
  const harness = await createHarness(undefined, false);
  requestEvent(harness, 62, "model/list", { includeHidden: false });
  await drain();
  const response = harness.emittedMessages.find((message) => message.message?.id === 62);
  assert.deepEqual(
    response.message.result.data.map((model) => model.model),
    ["gpt-5.4"],
    "third-party models must stay hidden until authoritative transport interception is ready",
  );
}

{
  const queuedModelList = {
    type: "mcp-request",
    hostId: "local",
    request: { id: 63, method: "model/list", params: { includeHidden: false } },
  };
  const harness = await createHarness(undefined, true, [queuedModelList]);
  await drain();
  const request = harness.nativeRequests.find((item) => item.id === 63);
  assert.ok(request, "pre-bridge model/list must be released after renderer injection is ready");
  assert.equal(request.params.includeHidden, true);
  const response = harness.emittedMessages.find((message) => message.message?.id === 63);
  assert.deepEqual(
    response.message.result.data.map((model) => model.model),
    ["gpt-5.4", "providerdeck:team_proxy:vendor:model:v2"],
    "queued initial model/list must receive third-party models",
  );
}

console.log("providerdeck renderer injection tests passed");
