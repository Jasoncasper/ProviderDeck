import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { CheckCircle2, XCircle, AlertTriangle } from "lucide-react";
import type { OverviewResult } from "@/lib/types";

interface HealthStatusProps {
  overview: OverviewResult | null;
}

function StatusIcon({ status }: { status: string }) {
  if (status === "ok" || status === "running") {
    return <CheckCircle2 className="h-4 w-4 text-green-500" />;
  }
  if (status === "failed" || status === "missing") {
    return <XCircle className="h-4 w-4 text-red-500" />;
  }
  return <AlertTriangle className="h-4 w-4 text-yellow-500" />;
}

function statusLabel(status: string): string {
  switch (status) {
    case "ok": return "正常";
    case "running": return "运行中";
    case "failed": return "异常";
    case "missing": return "未配置";
    default: return status || "未知";
  }
}

export function HealthStatus({ overview }: HealthStatusProps) {
  if (!overview) {
    return (
      <Card>
        <CardHeader><CardTitle className="text-lg">状态概览</CardTitle></CardHeader>
        <CardContent className="text-sm text-muted-foreground">暂无数据</CardContent>
      </Card>
    );
  }

  const codexApp = overview.codex_app;
  const latestLaunch = overview.latest_launch;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-lg">状态概览</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        {codexApp && (
          <div className="flex items-center justify-between text-sm">
            <span className="text-muted-foreground">Codex 应用路径</span>
            <span className="font-mono text-xs max-w-[200px] truncate" title={codexApp.path ?? ""}>
              {codexApp.path || "未检测到"}
            </span>
          </div>
        )}

        {latestLaunch && (
          <div className="flex items-center justify-between text-sm">
            <span className="text-muted-foreground">最近启动</span>
            <div className="flex items-center gap-1.5">
              <StatusIcon status={latestLaunch.status} />
              <Badge variant="outline" className="text-xs">
                {statusLabel(latestLaunch.status)}
              </Badge>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
