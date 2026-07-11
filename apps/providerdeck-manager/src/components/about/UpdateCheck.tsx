import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { AboutPanelHeader } from "./AboutPanelHeader";
import type { OverviewResult, UpdateResult } from "@/lib/types";

interface UpdateCheckProps {
  overview: OverviewResult | null;
  update: UpdateResult | null;
  onCheckUpdate: () => void;
  onPerformUpdate: () => void;
}

export function UpdateCheck({ overview, update, onCheckUpdate, onPerformUpdate }: UpdateCheckProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-lg">版本信息</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid grid-cols-2 gap-3">
          <div className="border rounded-lg bg-muted/30 p-3">
            <span className="text-xs text-muted-foreground">Codex 版本</span>
            <p className="font-semibold text-sm mt-1">{overview?.codex_version ?? "未检测到"}</p>
          </div>
          <div className="border rounded-lg bg-muted/30 p-3">
            <span className="text-xs text-muted-foreground">ProviderDeck 当前版本</span>
            <p className="font-semibold text-sm mt-1">
              {overview?.current_version ?? update?.currentVersion ?? "-"}
            </p>
          </div>
          <div className="border rounded-lg bg-muted/30 p-3">
            <span className="text-xs text-muted-foreground">最新版本</span>
            <p className="font-semibold text-sm mt-1">{update?.latestVersion ?? "未检查"}</p>
          </div>
          <div className="border rounded-lg bg-muted/30 p-3">
            <span className="text-xs text-muted-foreground">更新状态</span>
            <p className="font-semibold text-sm mt-1">{update?.status ?? "not_checked"}</p>
          </div>
        </div>
        <Textarea
          className="log-view"
          readOnly
          value={
            update?.releaseSummary ||
            update?.message ||
            "尚未检查 GitHub Release；更新会下载并启动安装包。"
          }
        />
        <div className="flex gap-2">
          <Button onClick={onCheckUpdate} size="sm">检查更新</Button>
          <Button variant="secondary" onClick={onPerformUpdate} size="sm">
            下载最新安装包
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
