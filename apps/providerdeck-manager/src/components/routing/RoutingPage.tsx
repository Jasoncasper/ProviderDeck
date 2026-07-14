import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirm } from "@tauri-apps/plugin-dialog";
import { Network, Image } from "lucide-react";
import { NoticeToast } from "@/components/shared/NoticeToast";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ProviderList } from "./ProviderList";
import { Select } from "@/components/ui/select";
import type { RoutingConfigPayload, SmartProvider, SmartRouterConfig } from "./types";

type Status = "ok" | "failed" | string;
type CommandResult<T> = T & { status: Status; message: string };

type FetchModelsPayload = {
  http_status: number;
  models: string[];
};

export default function RoutingPage() {
  const [config, setConfig] = useState<SmartRouterConfig | null>(null);
  const [activeTab, setActiveTab] = useState("models");
  const [notice, setNotice] = useState<{ type: "ok" | "error"; text: string } | null>(null);
  const [testingIndex, setTestingIndex] = useState<number | null>(null);
  const [expandedIndex, setExpandedIndex] = useState<number | null>(null);
  const [fetchingModelsIndex, setFetchingModelsIndex] = useState<number | null>(null);
  const [fetchedModelsMap, setFetchedModelsMap] = useState<Record<number, string[]>>({});
  const [advancedVisibleMap, setAdvancedVisibleMap] = useState<Record<number, boolean>>({});
  const autoSaveTimerRef = useRef<Record<number, ReturnType<typeof setTimeout>>>({});

  const loadConfig = useCallback(async () => {
    try {
      const result = await invoke<CommandResult<RoutingConfigPayload>>("load_routing_config");
      if (result.status === "ok") {
        setConfig(result.config);
      }
    } catch (e) {
      console.error("Failed to load routing config:", e);
    }
  }, []);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  const addProvider = () => {
    if (!config) return;
    const newProvider: SmartProvider = {
      id: "",
      name: "新模型",
      base_url: "",
      api_key: "",
      protocol: "chat_completions",
      enabled: true,
      supports_vision: false,
      use_full_url: false,
      target_model: "",
      model_pattern: "",
      builtin: false,
      user_agent: "",
      max_context: 0,
      supports_large_context: false,
      max_concurrent: 2,
    };
    setConfig({ ...config, providers: [...config.providers, newProvider] });
    setExpandedIndex(config.providers.length);
  };


  const saveConfig = async (updatedConfig?: SmartRouterConfig) => {
    const cfg = updatedConfig ?? config;
    if (!cfg) return;
    const missingProvider = cfg.providers.find((provider) => !provider.id.trim());
    if (missingProvider) {
      setNotice({ type: "error", text: "请先填写所有模型名称后再保存" });
      return;
    }
    try {
      const result = await invoke<CommandResult<RoutingConfigPayload>>("save_routing_config", {
        config: cfg,
      });
      setNotice({ type: result.status === "ok" ? "ok" : "error", text: result.message });
      if (result.status === "ok") {
        setConfig(result.config);
      }
    } catch (e: any) {
      setNotice({ type: "error", text: String(e) });
    }
  };


  const removeProvider = async (index: number) => {
    if (!config) return;
    const provider = config.providers[index];
    if (!provider || provider.builtin || provider.id === "openai") {
      setNotice({ type: "error", text: "内置 provider 不可删除" });
      return;
    }
    const confirmed = await confirm(
      `确定要删除模型「${provider.name}」吗？`,
      { title: "删除确认", kind: "warning" }
    );
    if (!confirmed) return;
    try {
      const result = await invoke<CommandResult<RoutingConfigPayload>>("delete_provider", {
        request: { providerId: provider.id },
      });
      setNotice({ type: result.status === "ok" ? "ok" : "error", text: result.message });
      if (result.status === "ok") {
        setConfig(result.config);
        setExpandedIndex((current) =>
          current === index ? null : current !== null && current > index ? current - 1 : current
        );
      }
    } catch (e: any) {
      setNotice({ type: "error", text: String(e) });
    }
  };

  const testProvider = async (index: number) => {
    if (!config) return;
    const provider = config.providers[index];
    if (!provider) return;
    setTestingIndex(index);
    try {
      const result = await invoke<CommandResult<any>>("test_smart_provider", { provider });
      console.log("[testProvider] raw result:", JSON.stringify(result));
      const httpStatus = result.httpStatus ?? result.http_status;
      const preview = result.responsePreview ?? result.response_preview;
      if (httpStatus >= 200 && httpStatus < 400) {
        setNotice({ type: "ok", text: `连接成功 - HTTP ${httpStatus}` });
      } else {
        setNotice({ type: "error", text: `连接失败 - HTTP ${httpStatus}: ${preview || result.message}` });
      }
    } catch (e: any) {
      console.error("[testProvider] error:", e);
      setNotice({ type: "error", text: String(e) });
    } finally {
      setTestingIndex(null);
    }
  };

  const fetchModels = async (index: number) => {
    if (!config) return;
    const provider = config.providers[index];
    if (!provider || !provider.base_url.trim()) return;
    setFetchingModelsIndex(index);
    try {
      const result = await invoke<CommandResult<FetchModelsPayload>>("fetch_provider_models", {
        provider,
      });
      if (result.status === "ok" && result.models.length > 0) {
        setFetchedModelsMap((prev) => ({ ...prev, [index]: result.models }));
        setNotice({ type: "ok", text: `拉取到 ${result.models.length} 个模型` });
      } else {
        setFetchedModelsMap((prev) => ({ ...prev, [index]: [] }));
        setNotice({ type: "error", text: result.message || "未拉取到模型列表，请手动填写" });
      }
    } catch (e: any) {
      setFetchedModelsMap((prev) => ({ ...prev, [index]: [] }));
      setNotice({ type: "error", text: `拉取失败: ${String(e)}，请手动填写上游模型名` });
    } finally {
      setFetchingModelsIndex(null);
    }
  };

  const toggleAdvanced = (index: number) => {
    setAdvancedVisibleMap((prev) => ({ ...prev, [index]: !prev[index] }));
  };

  const copyProvider = (index: number) => {
    if (!config) return;
    const provider = config.providers[index];
    if (!provider) return;
    const newProvider = {
      ...provider,
      id: "",
      name: `${provider.name} (副本)`,
    };
    setConfig({
      ...config,
      providers: [...config.providers, newProvider],
    });
    setExpandedIndex(config.providers.length);
  };

  const toggleEnabled = async (index: number) => {
    if (!config) return;
    const provider = config.providers[index];
    if (!provider) return;
    const updated = {
      ...config,
      providers: config.providers.map((p, i) =>
        i === index ? { ...p, enabled: !p.enabled } : p
      ),
    };
    setConfig(updated);
    await saveConfig(updated);
  };


  const updateProvider = (index: number, updates: Partial<SmartProvider>) => {
    if (!config) return;
    const providers = config.providers.map((p, i) =>
      i === index ? { ...p, ...updates } : p
    );
    setConfig({ ...config, providers });

    // 非内置 provider：输入停止 1.5s 后自动保存
    const provider = providers[index];
    if (provider && !provider.builtin && provider.id !== "openai" && provider.id.trim()) {
      if (autoSaveTimerRef.current[index]) clearTimeout(autoSaveTimerRef.current[index]);
      autoSaveTimerRef.current[index] = setTimeout(() => {
        void invoke("upsert_provider", { provider });
        setNotice({ type: "ok", text: "已自动保存" });
      }, 1500);
    }
  };

  if (!config) {
    return (
      <div className="flex items-center justify-center h-64">
        <p className="text-muted-foreground">加载配置中...</p>
      </div>
    );
  }

  const visionModels = config.providers.filter((m) => m.supports_vision && m.enabled);

  return (
    <div className="space-y-6">
      {notice && (
        <div className="fixed inset-0 z-50 flex items-start justify-center pt-[30vh] pointer-events-none">
          <div className="pointer-events-auto">
            <NoticeToast
              variant={notice.type === "ok" ? "success" : "error"}
              message={notice.text}
              onDismiss={() => setNotice(null)}
            />
          </div>
        </div>
      )}

      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold flex items-center gap-2">
          <Network className="h-5 w-5" /> Provider 配置
        </h2>
      </div>

      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList>
          <TabsTrigger value="models">模型</TabsTrigger>
          <TabsTrigger value="fallback">路由规则</TabsTrigger>
        </TabsList>

        <TabsContent value="models" className="mt-4">
          <ProviderList
            config={config}
            expandedIndex={expandedIndex}
            testingIndex={testingIndex}
            fetchingModelsIndex={fetchingModelsIndex}
            fetchedModelsMap={fetchedModelsMap}
            advancedVisibleMap={advancedVisibleMap}
            onToggleExpand={(idx) => {
              // 折叠卡片时自动保存
              setExpandedIndex(idx);
            }}
            onUpdate={updateProvider}
            onTest={testProvider}
            onFetchModels={fetchModels}
            onCopy={copyProvider}
            onToggleEnabled={toggleEnabled}
            onRemove={removeProvider}
            onAdd={addProvider}
            onToggleAdvanced={toggleAdvanced}
          />
        </TabsContent>

        <TabsContent value="fallback" className="mt-4 space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">回退配置</CardTitle>
              <CardDescription>
                当请求的模型没有匹配任何 provider 时，使用以下回退策略
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label>Fallback Provider（无匹配模型时使用）</Label>
                <Select
                  value={config.fallback_provider || ""}
                  onChange={(e) => {
                    const updated = { ...config!, fallback_provider: e.target.value };
                    setConfig(updated);
                    void saveConfig(updated);
                  }}
                >
                  <option value="">不设置 fallback</option>
                  {config.providers.filter((p) => p.enabled).map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.name} ({m.id})
                    </option>
                  ))}
                </Select>
                <p className="text-xs text-muted-foreground">
                  当选中的模型没有精确匹配或 pattern 匹配到任何 provider 时，使用此 provider 处理请求
                </p>
              </div>
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle className="text-lg flex items-center gap-2">
                <Image className="h-5 w-5" /> 多模态路由
              </CardTitle>
              <CardDescription>
                配置图片/视觉消息的路由规则：当选中模型不支持多模态时，自动回退到指定模型
              </CardDescription>
            </CardHeader>
            <CardContent>
              {visionModels.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  暂无支持多模态的模型。请先在"模型"标签中为模型勾选"支持多模态理解"。
                </p>
              ) : (
                <div className="space-y-2">
                  <Label>回退模型</Label>
                  <Select
                    value={config.vision_fallback_model}
                    onChange={(e) => {
                      const updated = { ...config!, vision_fallback_model: e.target.value };
                      setConfig(updated);
                      void saveConfig(updated);
                    }}
                  >
                    <option value="">不启用回退</option>
                    {visionModels.map((m) => (
                      <option key={m.id} value={m.id}>
                        {m.name} ({m.id})
                      </option>
                    ))}
                  </Select>

                  <p className="text-xs text-muted-foreground mt-2">
                    选择后，当请求包含图片但匹配的模型不支持多模态时，自动改用此模型处理
                  </p>
                </div>
              )}
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">图片压缩</CardTitle>
              <CardDescription>
                超过阈值的 base64 图片自动缩放为 JPEG，减少 token 消耗
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <label className="flex items-center gap-2 text-sm cursor-pointer">
                <input
                  type="checkbox"
                  checked={config.image_auto_compress}
                  onChange={(e) => {
                    const updated = { ...config!, image_auto_compress: e.target.checked };
                    setConfig(updated);
                    void saveConfig(updated);
                  }}
                />
                自动压缩超大图片
              </label>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <Label className="text-xs">触发阈值（KB）</Label>
                  <Input
                    type="number"
                    value={config.image_max_size_kb}
                    onChange={(e) => {
                      const updated = { ...config!, image_max_size_kb: parseInt(e.target.value) || 0 };
                      setConfig(updated);
                      void saveConfig(updated);
                    }}
                    className="h-8 text-sm"
                    min={0}
                    max={10000}
                  />
                  <p className="text-xs text-muted-foreground mt-1">留空则默认 100 KB</p>
                </div>
                <div>
                  <Label className="text-xs">最大尺寸（px，最长边）</Label>
                  <Input
                    type="number"
                    value={config.image_max_dimension}
                    onChange={(e) => {
                      const updated = { ...config!, image_max_dimension: parseInt(e.target.value) || 1024 };
                      setConfig(updated);
                      void saveConfig(updated);
                    }}
                    className="h-8 text-sm"
                    min={128}
                    max={4096}
                  />
                </div>
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

    </div>
  );
}
