import { useState } from "react";
import { Activity, FileText } from "lucide-react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { LogsPanel } from "./LogsPanel";
import { DiagnosticsPanel } from "./DiagnosticsPanel";
import type { LogsResult, DiagnosticsResult } from "@/lib/types";

type PanelTab = "logs" | "diagnostics";

interface LogDiagnosticsTabsProps {
  logs: LogsResult | null;
  diagnostics: DiagnosticsResult | null;
  onRefreshLogs: () => void;
  onCopyLogs: () => void;
  onRefreshDiagnostics: () => void;
  onCopyDiagnostics: () => void;
}

export function LogDiagnosticsTabs({
  logs,
  diagnostics,
  onRefreshLogs,
  onCopyLogs,
  onRefreshDiagnostics,
  onCopyDiagnostics,
}: LogDiagnosticsTabsProps) {
  const [activeTab, setActiveTab] = useState<PanelTab>("logs");

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <h2 className="text-2xl font-bold">日志诊断</h2>
        <p className="text-sm text-muted-foreground">查看日志并生成诊断报告。</p>
      </div>

      <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as PanelTab)}>
        <TabsList>
          <TabsTrigger value="logs">
            <Activity className="h-4 w-4 mr-1" /> 最近日志
          </TabsTrigger>
          <TabsTrigger value="diagnostics">
            <FileText className="h-4 w-4 mr-1" /> 诊断报告
          </TabsTrigger>
        </TabsList>

        <TabsContent value="logs">
          <LogsPanel logs={logs} onRefresh={onRefreshLogs} onCopy={onCopyLogs} />
        </TabsContent>
        <TabsContent value="diagnostics">
          <DiagnosticsPanel
            diagnostics={diagnostics}
            onRefresh={onRefreshDiagnostics}
            onCopy={onCopyDiagnostics}
          />
        </TabsContent>
      </Tabs>
    </div>
  );
}
