import React from "react";
import {
  ChevronDown,
  ChevronRight,
  CheckCircle2,
  Copy,
  Download,
  FlaskConical,
  Image,
  Trash2,
  XCircle,
  Search,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Select } from "@/components/ui/select";
import type { SmartProvider, ProviderProtocol } from "./types";

interface ProviderCardProps {
  provider: SmartProvider;
  index: number;
  isExpanded: boolean;
  isTesting: boolean;
  isFetchingModels: boolean;
  fetchedModels: string[];
  showAdvanced: boolean;
  onToggleExpand: (index: number | null) => void;
  onUpdate: (index: number, updates: Partial<SmartProvider>) => void;
  onTest: (index: number) => void;
  onFetchModels: (index: number) => void;
  onCopy: (index: number) => void;
  onToggleEnabled: (index: number) => void;
  onRemove: (index: number) => void;
  onToggleAdvanced: () => void;
}

const ModelSelector = React.memo(function ModelSelector({
  value,
  models,
  isFetching,
  baseUrl,
  onChange,
  onFetch,
}: {
  value: string;
  models: string[];
  isFetching: boolean;
  baseUrl: string;
  onChange: (id: string) => void;
  onFetch: () => void;
}) {
  const [search, setSearch] = React.useState("");
  const [open, setOpen] = React.useState(false);
  const containerRef = React.useRef<HTMLDivElement>(null);

  // Close on outside click
  React.useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    if (open) {
      document.addEventListener("mousedown", handler);
    }
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const hasModels = models.length > 0;
  const filtered = hasModels
    ? models.filter((m) => m.toLowerCase().includes(search.toLowerCase()))
    : [];
  const showCurrent = value && !models.includes(value);

  return (
    <div className="flex gap-1" ref={containerRef}>
      <div className="relative flex-1">
        {hasModels ? (
          <>
            <div className="flex items-center gap-1">
              <div className="relative flex-1">
                <Input
                  value={open ? search : value}
                  onChange={(e) => {
                    if (open) {
                      setSearch(e.target.value);
                    } else {
                      onChange(e.target.value);
                    }
                  }}
                  onFocus={() => { setOpen(true); setSearch(""); }}
                  className="h-8 text-sm pr-7"
                  placeholder="搜索或手动输入..."
                />
                {value && (
                  <button
                    className="absolute right-1 top-1/2 -translate-y-1/2 p-1 opacity-40 hover:opacity-100"
                    onClick={(e) => { e.stopPropagation(); onChange(""); setSearch(""); }}
                    type="button"
                  >
                    <X className="h-3 w-3" />
                  </button>
                )}
              </div>
            </div>
            {open && (
              <div className="absolute z-50 mt-1 w-full bg-popover border rounded-md shadow-md max-h-48 overflow-auto">
                {showCurrent && (
                  <button
                    type="button"
                    className="w-full text-left px-3 py-1.5 text-sm hover:bg-accent text-muted-foreground"
                    onClick={() => { onChange(value); setOpen(false); setSearch(""); }}
                  >
                    {value} (当前)
                  </button>
                )}
                {search && (
                  <button
                    type="button"
                    className="w-full text-left px-3 py-1.5 text-sm hover:bg-accent italic text-muted-foreground"
                    onClick={() => { onChange(search); setOpen(false); setSearch(""); }}
                  >
                    使用 "{search}" 作为模型ID
                  </button>
                )}
                {filtered.length === 0 && !showCurrent && !search && (
                  <p className="px-3 py-2 text-sm text-muted-foreground">无匹配模型</p>
                )}
                {filtered.map((model) => (
                  <button
                    key={model}
                    type="button"
                    className={`w-full text-left px-3 py-1.5 text-sm hover:bg-accent ${model === value ? "font-medium bg-accent/50" : ""}`}
                    onClick={() => { onChange(model); setOpen(false); setSearch(""); }}
                  >
                    {model}
                  </button>
                ))}
              </div>
            )}
          </>
        ) : (
          <Input
            value={value}
            onChange={(e) => onChange(e.target.value)}
            className="h-8 text-sm flex-1"
            placeholder="openai/gpt-5"
          />
        )}
      </div>
      <Button
        variant="outline"
        size="sm"
        className="h-8 px-2 text-xs shrink-0"
        title="从上游拉取可用模型列表"
        onClick={(e) => { e.stopPropagation(); onFetch(); }}
        disabled={isFetching || !baseUrl.trim()}
      >
        <Download className={`h-3 w-3 mr-1 ${isFetching ? "animate-spin" : ""}`} />
        拉取
      </Button>
    </div>
  );
});

export function ProviderCard({
  provider,
  index,
  isExpanded,
  isTesting,
  onToggleExpand,
  onUpdate,
  onTest,
  onFetchModels,
  onCopy,
  onToggleEnabled,
  onRemove,
  isFetchingModels,
  fetchedModels,
  showAdvanced,
  onToggleAdvanced,
}: ProviderCardProps) {
  const isNew = !provider.id;
  const isBuiltin = provider.builtin || provider.id === "openai";

  return (
    <Card className={isNew ? "border-blue-300" : ""}>
      <CardHeader
        className={`pb-3 rounded-t-lg ${isExpanded && !isBuiltin ? "" : "rounded-b-lg"} ${isBuiltin ? "" : "cursor-pointer hover:bg-muted/30"}`}
        onClick={isBuiltin ? undefined : () => onToggleExpand(isExpanded ? null : index)}
      >
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2 min-w-0">
            {isBuiltin ? (
              <span className="h-4 w-4 shrink-0" />
            ) : isExpanded ? (
              <ChevronDown className="h-4 w-4 text-muted-foreground shrink-0" />
            ) : (
              <ChevronRight className="h-4 w-4 text-muted-foreground shrink-0" />
            )}
            <CardTitle className="text-base min-w-0">
              {isBuiltin ? (
                <span className="truncate">{provider.name}</span>
              ) : isExpanded ? (
                <Input
                  value={provider.name}
                  onChange={(e) => onUpdate(index, { name: e.target.value })}
                  onClick={(e) => e.stopPropagation()}
                  className="h-7 text-base font-semibold border-none p-0 w-auto"
                />
              ) : (
                <span className="truncate">{provider.name}</span>
              )}
            </CardTitle>
            <span className="text-xs text-muted-foreground truncate hidden sm:inline">
              {provider.id}
            </span>
            {provider.enabled ? (
              <Badge variant="default" className="bg-green-100 dark:bg-green-900 text-green-800 dark:text-green-200 shrink-0">
                启用
              </Badge>
            ) : (
              <Badge variant="secondary" className="shrink-0">
                禁用
              </Badge>
            )}
            {provider.supports_vision && (
              <Badge variant="outline" className="shrink-0">
                <Image className="h-3 w-3 inline mr-1" />
                多模态
              </Badge>
            )}
          </div>
          <div className="flex gap-1 shrink-0" onClick={(e) => e.stopPropagation()}>
            {isBuiltin ? (
              <Badge variant="outline" className="text-xs border-blue-300 dark:border-blue-700 text-blue-600 dark:text-blue-400">
                内置
              </Badge>
            ) : (
              <>
                <Button
                  variant="ghost"
                  size="sm"
                  title="测试连接"
                  onClick={() => onTest(index)}
                  disabled={isTesting}
                >
                  <FlaskConical className={`h-4 w-4 ${isTesting ? "animate-spin" : ""}`} />
                  {isTesting ? " 测试中" : ""}
                </Button>
                <span className="w-px h-5 bg-border self-center mx-0.5" />
                <Button variant="ghost" size="sm" title="复制模型" onClick={() => onCopy(index)}>
                  <Copy className="h-4 w-4" />
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => onToggleEnabled(index)}
                  title={provider.enabled ? "禁用" : "启用"}
                >
                  {provider.enabled ? (
                    <CheckCircle2 className="h-4 w-4 text-green-600" />
                  ) : (
                    <XCircle className="h-4 w-4 text-muted-foreground" />
                  )}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => onRemove(index)}
                  title="删除模型"
                >
                  <Trash2 className="h-4 w-4 text-red-600" />
                </Button>
              </>
            )}
          </div>
        </div>
      </CardHeader>
      {isBuiltin ? (
        <CardContent className="text-sm text-muted-foreground">
          <p>
            这是系统内置的默认 provider，始终保留在列表首位，作为 fallback 默认目标使用。
          </p>
          <p>
            不可编辑、不可删除、不可展开。模型通过 <code>{provider.model_pattern}</code> 模式自动匹配。
          </p>
          <p>Base URL: {provider.base_url}</p>
        </CardContent>
      ) : (
        <div className={`grid transition-all duration-200 ease-out ${isExpanded ? "grid-rows-[1fr] opacity-100" : "grid-rows-[0fr] opacity-0"}`}>
          <div className="overflow-hidden">
            <CardContent className="space-y-3">
              <div className="grid grid-cols-2 gap-3">
            <div>
              <Label className="text-xs">模型名称（模型ID）</Label>
              <ModelSelector
                value={provider.id}
                models={fetchedModels}
                isFetching={isFetchingModels}
                baseUrl={provider.base_url}
                onChange={(id) => onUpdate(index, { id })}
                onFetch={() => onFetchModels(index)}
              />
            </div>
            <div>
              <Label className="text-xs">协议</Label>
              <Select
                value={provider.protocol}
                onChange={(e) =>
                  onUpdate(index, { protocol: e.target.value as ProviderProtocol })
                }
              >
                <option value="chat_completions">Chat Completions</option>
                <option value="responses">Responses</option>
                <option value="anthropic">Anthropic</option>
              </Select>
            </div>
            <div className="col-span-2">
              <Label className="text-xs">Base URL</Label>
              <div className="flex items-center gap-2">
                <Input
                  value={provider.base_url}
                  onChange={(e) => onUpdate(index, { base_url: e.target.value })}
                  placeholder="https://api.openai.com/v1"
                  className="h-8 text-sm flex-1"
                />
                <label className="flex items-center gap-1 text-xs cursor-pointer whitespace-nowrap">
                  <input
                    type="checkbox"
                    checked={provider.use_full_url}
                    onChange={(e) => onUpdate(index, { use_full_url: e.target.checked })}
                  />
                  使用完整 URL
                </label>
              </div>
            </div>
            <div className="col-span-2">
              <Label className="text-xs">API Key</Label>
              <Input
                type="text"
                value={provider.api_key}
                onChange={(e) => onUpdate(index, { api_key: e.target.value })}
                placeholder="sk-..."
                className="h-8 text-sm"
              />
            </div>
          </div>
          <label className="flex items-center gap-2 text-sm cursor-pointer">
            <input
              type="checkbox"
              checked={provider.supports_vision}
              onChange={(e) => onUpdate(index, { supports_vision: e.target.checked })}
            />
            <Image className="h-4 w-4" /> 支持多模态理解（图片/视觉）
          </label>

          <div className="border-t pt-3 mt-3">
            <button
              type="button"
              className="flex items-center gap-1 text-xs font-medium text-muted-foreground hover:text-foreground transition-colors w-full text-left"
              onClick={(e) => { e.stopPropagation(); onToggleAdvanced(); }}
            >
              {showAdvanced ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
              高级模型配置
            </button>
            {showAdvanced && (
              <div className="grid grid-cols-2 gap-3 mt-2">
                <div>
                  <Label className="text-xs">上游模型名（留空则用模型 ID）</Label>
                  <Input
                    value={provider.target_model}
                    onChange={(e) => onUpdate(index, { target_model: e.target.value })}
                    className="h-8 text-sm"
                  />
                </div>
                <div>
                  <Label className="text-xs">自定义 User-Agent</Label>
                  <Input
                    value={provider.user_agent}
                    onChange={(e) => onUpdate(index, { user_agent: e.target.value })}
                    className="h-8 text-sm"
                  />
                </div>
                <div>
                  <Label className="text-xs">最大上下文（0=默认）</Label>
                  <Input
                    type="number"
                    value={provider.max_context}
                    onChange={(e) =>
                      onUpdate(index, {
                        max_context: parseInt(e.target.value) || 0,
                      })
                    }
                    className="h-8 text-sm"
                  />
                </div>
                <div className="flex items-end pb-1">
                  <label className="flex items-center gap-2 text-sm cursor-pointer">
                    <input
                      type="checkbox"
                      checked={provider.supports_large_context}
                      onChange={(e) =>
                        onUpdate(index, { supports_large_context: e.target.checked })
                      }
                    />
                    支持大上下文
                  </label>
                </div>
                <div>
                  <Label className="text-xs">最大并发（0=不限制）</Label>
                  <Input
                    type="number"
                    min={0}
                    value={provider.max_concurrent}
                    onChange={(e) => onUpdate(index, { max_concurrent: parseInt(e.target.value) || 0 })}
                    className="h-8 text-sm"
                  />
                </div>
              </div>
            )}
          </div>
          </CardContent>
          </div>
        </div>
      )}
    </Card>
  );
}
