import { Activity, Cable, RefreshCw, Server } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import type { OverviewResult, RuntimeStatus, SwitchJournalResult } from "@/lib/types";

interface LaunchPageProps {
  overview: OverviewResult | null;
  runtime: RuntimeStatus | null;
  journal: SwitchJournalResult | null;
  onRestart: () => void;
  onRecover: () => void;
  onSafeExit: () => void;
}

function statusBadge(ok: boolean, online: string, offline: string) {
  return <Badge variant={ok ? "default" : "secondary"}>{ok ? online : offline}</Badge>;
}

export function LaunchPage({ overview, runtime, journal, onRestart, onRecover, onSafeExit }: LaunchPageProps) {
  const phase = journal?.record?.phase ?? runtime?.switchPhase ?? "stable";
  const needsRecovery = phase === "recovery_required";

  return (
    <div className="mx-auto max-w-4xl space-y-6 py-4">
      <div>
        <h2 className="text-xl font-semibold">Runtime</h2>
        <p className="mt-1 text-sm text-muted-foreground">ChatGPT/Codex 连接与 provider 切换状态</p>
      </div>

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Card><CardContent className="space-y-2 pt-4"><Cable className="h-4 w-4 text-muted-foreground"/><p className="text-sm">App server</p>{statusBadge(Boolean(runtime?.appServerConnected), "已连接", "未连接")}</CardContent></Card>
        <Card><CardContent className="space-y-2 pt-4"><Activity className="h-4 w-4 text-muted-foreground"/><p className="text-sm">CDP bridge</p>{statusBadge(Boolean(runtime?.bridgeInjected), "已注入", "未注入")}</CardContent></Card>
        <Card><CardContent className="space-y-2 pt-4"><Server className="h-4 w-4 text-muted-foreground"/><p className="text-sm">Helper</p>{statusBadge(Boolean(runtime?.helperHealthy), `端口 ${runtime?.helperPort}`, "未运行")}</CardContent></Card>
        <Card><CardContent className="space-y-2 pt-4"><RefreshCw className="h-4 w-4 text-muted-foreground"/><p className="text-sm">切换状态</p><Badge variant={needsRecovery ? "secondary" : "outline"} className={needsRecovery ? "text-destructive" : undefined}>{phase}</Badge></CardContent></Card>
      </div>

      <div className="flex flex-wrap gap-3">
        <Button onClick={onRestart}><RefreshCw className="mr-2 h-4 w-4"/>启动或重启 ChatGPT</Button>
        <Button onClick={onRecover} variant="outline" disabled={!runtime?.appServerConnected}>重新注入并恢复 runtime</Button>
        <Button onClick={onSafeExit} variant="outline">安全退出</Button>
      </div>

      {journal?.record ? (
        <Card>
          <CardContent className="space-y-2 pt-4 text-sm">
            <div className="flex justify-between"><span className="text-muted-foreground">任务</span><span className="font-mono text-xs">{journal.record.threadId ?? "-"}</span></div>
            <div className="flex justify-between"><span className="text-muted-foreground">目标</span><span>{journal.record.target?.providerId ?? "-"} / {journal.record.target?.model ?? "-"}</span></div>
            {journal.record.error ? <p className="text-destructive">{journal.record.error}</p> : null}
          </CardContent>
        </Card>
      ) : null}

      <p className="text-xs text-muted-foreground">ProviderDeck {overview?.current_version ?? "-"}。官方模型请求保持直连，仅代理模型经过本地 helper。</p>
    </div>
  );
}
