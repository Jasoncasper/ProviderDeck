(function () {
  "use strict";

  var bridge = window.__providerDeckBridge;
  if (typeof bridge !== "function" || window.__providerDeckInstalled) return;
  window.__providerDeckInstalled = true;

  var originalDispatch = window.dispatchEvent.bind(window);
  var catalog = { status: "loading", models: [], providers: {} };
  var catalogPromise = null;
  var officialModels = new Set();
  var modelListRequestIds = new Set();
  var requestMetadata = new Map();
  var internalRequests = new Map();
  var threadBindings = new Map();
  var threadStatuses = new Map();
  var queuedTurns = new Map();
  var pendingBindings = new Map();
  var hostId = "local";
  var internalSequence = 0;

  function loadCatalog() {
    if (catalogPromise) return catalogPromise;
    if (window.__PROVIDERDECK_BOOTSTRAP__ && window.__PROVIDERDECK_BOOTSTRAP__.status === "ok") {
      catalog = window.__PROVIDERDECK_BOOTSTRAP__;
      catalogPromise = Promise.resolve(catalog);
      return catalogPromise;
    }
    catalogPromise = Promise.resolve(bridge("/providerdeck/catalog", {}))
      .then(function (value) {
        if (value && value.status === "ok") catalog = value;
        return catalog;
      })
      .catch(function () { return catalog; });
    return catalogPromise;
  }

  function parseSelection(selection) {
    if (typeof selection !== "string" || selection.indexOf("providerdeck:") !== 0) return null;
    var scoped = selection.slice("providerdeck:".length);
    var delimiter = scoped.indexOf(":");
    if (delimiter <= 0 || delimiter === scoped.length - 1) return null;
    var providerId = scoped.slice(0, delimiter);
    if (!/^[A-Za-z0-9_.-]+$/.test(providerId)) return null;
    return { providerId: providerId, model: scoped.slice(delimiter + 1) };
  }

  function officialTarget(model) {
    if (!officialModels.has(model) && !/^(gpt-|o[1-9]|codex-)/.test(model || "")) return null;
    return { model: model, providerId: "openai", runtimeProviderId: "openai", source: "official" };
  }

  function targetForSelection(selection) {
    var parsed = parseSelection(selection);
    if (!parsed) return officialTarget(selection);
    var provider = catalog.providers && catalog.providers[parsed.providerId];
    if (!provider) return null;
    return {
      model: parsed.model,
      providerId: parsed.providerId,
      runtimeProviderId: provider.runtimeProviderId,
      source: "proxy",
      provider: provider,
    };
  }

  function providerConfig(target) {
    if (!target || target.source === "official") return null;
    var key = "model_providers." + target.runtimeProviderId;
    var config = {};
    config[key] = {
      name: target.provider.name,
      base_url: target.provider.baseUrl,
      wire_api: "responses",
      requires_openai_auth: false,
      experimental_bearer_token: target.provider.bearerToken || "",
    };
    return config;
  }

  function applyTarget(params, target) {
    var next = Object.assign({}, params || {}, {
      model: target.model,
      modelProvider: target.runtimeProviderId,
    });
    var config = providerConfig(target);
    if (config) next.config = Object.assign({}, next.config || {}, config);
    return next;
  }

  function modelDescriptor(model) {
    return {
      model: model.selection,
      id: model.selection,
      slug: model.selection,
      name: model.displayName,
      displayName: model.displayName,
      description: model.description,
      hidden: false,
      isDefault: false,
      defaultReasoningEffort: model.supportedReasoningEfforts.indexOf("medium") >= 0 ? "medium" : (model.supportedReasoningEfforts[0] || "medium"),
      supportedReasoningEfforts: model.supportedReasoningEfforts.map(function (effort) {
        return { reasoningEffort: effort, description: effort + " effort" };
      }),
    };
  }

  function patchModelArray(models) {
    if (!Array.isArray(models) || !models.every(function (item) { return item && typeof item.model === "string"; })) return false;
    var existing = new Set();
    models.forEach(function (item) {
      existing.add(item.model);
      if (item.model.indexOf("providerdeck:") !== 0) officialModels.add(item.model);
    });
    (catalog.models || []).forEach(function (model) {
      if (!existing.has(model.selection)) models.push(modelDescriptor(model));
    });
    return true;
  }

  function patchModelListResult(result) {
    if (!result || typeof result !== "object") return false;
    if (patchModelArray(result.data)) return true;
    if (patchModelArray(result.models)) return true;
    return false;
  }

  function responseEnvelope(data) {
    if (!data || data.type !== "mcp-response") return null;
    return data.message || data.response || null;
  }

  function notificationEnvelope(data) {
    if (!data || data.type !== "mcp-notification") return null;
    return data.message || data.notification || null;
  }

  function sendInternal(method, params) {
    return new Promise(function (resolve, reject) {
      var id = "providerdeck-" + (++internalSequence);
      internalRequests.set(id, { resolve: resolve, reject: reject });
      originalDispatch(new CustomEvent("codex-message-from-view", {
        detail: {
          type: "mcp-request",
          hostId: hostId,
          request: { id: id, method: method, params: params || {} },
        },
      }));
    });
  }

  function journal(record) {
    window.__PROVIDERDECK_RUNTIME_STATE__ = record;
    return Promise.resolve(bridge("/providerdeck/switch-journal/save", record)).catch(function () {});
  }

  function bindingFromTarget(target) {
    return { model: target.model, providerId: target.runtimeProviderId };
  }

  function verifyResume(result, threadId, target) {
    return !!result
      && result.thread
      && result.thread.id === threadId
      && result.model === target.model
      && result.modelProvider === target.runtimeProviderId;
  }

  function resumeParams(threadId, target) {
    return applyTarget({ threadId: threadId }, target);
  }

  async function performSwitch(threadId, target) {
    var original = threadBindings.get(threadId);
    if (!original) {
      var current = await sendInternal("thread/read", { threadId: threadId, includeTurns: false });
      var currentThread = current && current.thread;
      var currentModel = current && current.model || currentThread && currentThread.model;
      var currentProvider = current && current.modelProvider || currentThread && currentThread.modelProvider;
      if (currentModel && currentProvider) {
        original = { model: currentModel, providerId: currentProvider };
        threadBindings.set(threadId, original);
      }
    }
    if (original && original.model === target.model && original.providerId === target.runtimeProviderId) return;

    await journal({
      phase: "switching",
      threadId: threadId,
      original: original || null,
      target: bindingFromTarget(target),
      error: null,
    });
    try {
      await sendInternal("thread/unsubscribe", { threadId: threadId });
      var resumed = await sendInternal("thread/resume", resumeParams(threadId, target));
      if (!verifyResume(resumed, threadId, target)) throw new Error("provider switch verification failed");
      threadBindings.set(threadId, bindingFromTarget(target));
      await journal({ phase: "stable", threadId: threadId, original: original || null, target: bindingFromTarget(target), error: null });
      return;
    } catch (targetError) {
      await journal({
        phase: "rolling_back",
        threadId: threadId,
        original: original || null,
        target: bindingFromTarget(target),
        error: String(targetError && targetError.message || targetError),
      });
      if (!original) {
        await journal({ phase: "recovery_required", threadId: threadId, original: null, target: bindingFromTarget(target), error: "original binding unavailable" });
        throw targetError;
      }
      try {
        await sendInternal("thread/unsubscribe", { threadId: threadId });
      } catch (_) {}
      try {
        var rollbackTarget = {
          model: original.model,
          runtimeProviderId: original.providerId,
          providerId: original.providerId,
          source: original.providerId === "openai" ? "official" : "proxy",
          provider: null,
        };
        if (rollbackTarget.source === "proxy") {
          var providerId = original.providerId.replace(/^providerdeck-/, "");
          rollbackTarget.providerId = providerId;
          rollbackTarget.provider = catalog.providers && catalog.providers[providerId];
        }
        var rolledBack = await sendInternal("thread/resume", resumeParams(threadId, rollbackTarget));
        if (!verifyResume(rolledBack, threadId, rollbackTarget)) throw new Error("provider rollback verification failed");
        threadBindings.set(threadId, original);
        await journal({ phase: "failed", threadId: threadId, original: original, target: bindingFromTarget(target), rolledBack: true, error: String(targetError && targetError.message || targetError) });
      } catch (rollbackError) {
        await journal({ phase: "recovery_required", threadId: threadId, original: original, target: bindingFromTarget(target), error: String(rollbackError && rollbackError.message || rollbackError) });
      }
      throw targetError;
    }
  }

  function emitRequestError(request, error) {
    originalDispatch(new MessageEvent("message", {
      data: {
        type: "mcp-response",
        message: { id: request.id, error: { code: -32000, message: String(error && error.message || error) } },
      },
    }));
  }

  async function releaseTurn(event, request, target) {
    try {
      await performSwitch(request.params.threadId, target);
      request.params = Object.assign({}, request.params, { model: target.model });
      originalDispatch(event);
    } catch (error) {
      emitRequestError(request, error);
    }
  }

  async function handleTurnStart(event, request) {
    await loadCatalog();
    var target = targetForSelection(request.params && request.params.model);
    if (!target) return originalDispatch(event);
    var threadId = request.params.threadId;
    if (threadStatuses.get(threadId) === "active") {
      var superseded = queuedTurns.get(threadId);
      if (superseded) emitRequestError(superseded.request, new Error("model switch superseded by a newer selection"));
      queuedTurns.set(threadId, { event: event, request: request, target: target });
      pendingBindings.set(threadId, target);
      await journal({ phase: "pending", threadId: threadId, original: threadBindings.get(threadId) || null, target: bindingFromTarget(target), error: null });
      return true;
    }
    await releaseTurn(event, request, target);
    return true;
  }

  async function handleThreadStart(event, request) {
    await loadCatalog();
    var target = targetForSelection(request.params && request.params.model);
    if (target) request.params = applyTarget(request.params, target);
    return originalDispatch(event);
  }

  async function flushPending(threadId) {
    threadStatuses.set(threadId, "idle");
    var queued = queuedTurns.get(threadId);
    if (queued) {
      queuedTurns.delete(threadId);
      pendingBindings.delete(threadId);
      await releaseTurn(queued.event, queued.request, queued.target);
      return;
    }
    var target = pendingBindings.get(threadId);
    if (target) {
      pendingBindings.delete(threadId);
      try { await performSwitch(threadId, target); } catch (_) {}
    }
  }

  function trackResponse(request, result) {
    if (!request || !result) return;
    if ((request.method === "thread/start" || request.method === "thread/resume") && result.thread && result.thread.id) {
      if (result.model && result.modelProvider) {
        threadBindings.set(result.thread.id, { model: result.model, providerId: result.modelProvider });
      }
      if (result.thread.status && result.thread.status.type) threadStatuses.set(result.thread.id, result.thread.status.type);
    }
  }

  function onMessage(event) {
    var envelope = responseEnvelope(event && event.data);
    if (envelope) {
      var id = String(envelope.id);
      var internal = internalRequests.get(id);
      if (internal) {
        internalRequests.delete(id);
        if (envelope.error) internal.reject(new Error(envelope.error.message || "app-server request failed"));
        else internal.resolve(envelope.result);
        return;
      }
      var metadata = requestMetadata.get(id);
      if (metadata) {
        requestMetadata.delete(id);
        if (modelListRequestIds.has(id)) {
          modelListRequestIds.delete(id);
          patchModelListResult(envelope.result);
        }
        trackResponse(metadata, envelope.result);
      }
      return;
    }
    var notification = notificationEnvelope(event && event.data);
    if (!notification) return;
    var params = notification.params || {};
    if (notification.method === "thread/status/changed" && params.threadId) {
      var status = params.status && params.status.type || "unknown";
      threadStatuses.set(params.threadId, status);
      if ((status === "idle" || status === "interrupted") && (queuedTurns.has(params.threadId) || pendingBindings.has(params.threadId))) {
        void flushPending(params.threadId);
      }
    }
    if (notification.method === "turn/completed" && params.threadId) {
      void flushPending(params.threadId);
    }
  }

  window.addEventListener("message", onMessage, true);
  window.dispatchEvent = function providerDeckDispatch(event) {
    var detail = event && event.detail;
    var request = detail && detail.type === "mcp-request" && detail.request;
    if (!event || event.type !== "codex-message-from-view" || !request) return originalDispatch(event);
    hostId = detail.hostId || hostId;
    requestMetadata.set(String(request.id), { method: request.method, params: request.params || {} });
    if (request.method === "model/list") {
      request.params = Object.assign({}, request.params || {}, { includeHidden: true });
      modelListRequestIds.add(String(request.id));
      return originalDispatch(event);
    }
    if (request.method === "thread/start") {
      void handleThreadStart(event, request);
      return true;
    }
    if (request.method === "turn/start") {
      void handleTurnStart(event, request);
      return true;
    }
    return originalDispatch(event);
  };

  void loadCatalog();
})();
