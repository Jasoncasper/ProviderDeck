import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { AboutPanelHeader } from "./AboutPanelHeader";
import type { DiagnosticsResult } from "@/lib/types";
import { ClipboardList } from "lucide-react";

interface DiagnosticsPanelProps {
  diagnostics: DiagnosticsResult | null;
  onRefresh: () => void;
  onCopy: () => void;
}

export function DiagnosticsPanel({ diagnostics, onRefresh, onCopy }: DiagnosticsPanelProps) {
  const hasReport = !!(diagnostics?.report);

  return (
    <div className="mt-4 space-y-3">
      <div className="flex items-center justify-between gap-4">
        <AboutPanelHeader title="诊断报告" description="包含版本、路径、设置和平台信息" />
        <div className="flex gap-2">
          <Button size="sm" onClick={onRefresh}>重新生成</Button>
          <Button size="sm" variant="secondary" onClick={onCopy}>
            复制报告
          </Button>
        </div>
      </div>
      {hasReport ? (
        <Textarea
          className="log-view tall"
          readOnly
          value={diagnostics!.report}
        />
      ) : (
        <div className="flex flex-col items-center justify-center gap-2 py-8 border rounded-lg border-dashed text-muted-foreground">
          <ClipboardList className="h-8 w-8" />
          <span>尚未生成诊断报告</span>
          <Button variant="outline" size="sm" onClick={onRefresh}>点击生成</Button>
        </div>
      )}
    </div>
  );
}
