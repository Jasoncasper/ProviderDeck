import { Button } from "@/components/ui/button";
import { splitLogLines } from "@/lib/toml";
import { AboutPanelHeader } from "./AboutPanelHeader";
import type { LogsResult } from "@/lib/types";
import { FileX } from "lucide-react";

interface LogsPanelProps {
  logs: LogsResult | null;
  onRefresh: () => void;
  onCopy: () => void;
}

export function LogsPanel({ logs, onRefresh, onCopy }: LogsPanelProps) {
  const lines = splitLogLines(logs?.text ?? "");

  return (
    <div className="mt-4 space-y-3">
      <div className="flex items-center justify-between gap-4">
        <AboutPanelHeader title="最近日志" description={logs?.path ?? ""} />
        <div className="flex gap-2">
          <Button size="sm" onClick={onRefresh}>刷新</Button>
          <Button size="sm" variant="secondary" onClick={onCopy}>
            复制
          </Button>
        </div>
      </div>
      <div className="log-lines">
        {lines.length ? (
          lines.map((line, index) => (
            <div className="log-line" key={`${index}-${line.slice(0, 12)}`}>
              <span>{index + 1}</span>
              <code>{line || " "}</code>
            </div>
          ))
        ) : (
          <div className="flex flex-col items-center justify-center gap-2 py-8 text-muted-foreground">
            <FileX className="h-8 w-8" />
            <span>暂无日志</span>
            <Button variant="outline" size="sm" onClick={onRefresh}>点击刷新</Button>
          </div>
        )}
      </div>
    </div>
  );
}
