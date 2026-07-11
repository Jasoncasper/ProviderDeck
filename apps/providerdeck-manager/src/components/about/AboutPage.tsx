import { UpdateCheck } from "./UpdateCheck";
import { HealthStatus } from "./HealthStatus";
import type { OverviewResult, UpdateResult } from "@/lib/types";

interface AboutPageProps {
  overview: OverviewResult | null;
  update: UpdateResult | null;
  onCheckUpdate: () => void;
  onPerformUpdate: () => void;
}

export function AboutPage({
  overview,
  update,
  onCheckUpdate,
  onPerformUpdate,
}: AboutPageProps) {
  return (
    <div className="space-y-6 max-w-3xl mx-auto">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <h2 className="text-2xl font-bold">关于</h2>
        <p className="text-sm text-muted-foreground">版本、状态与更新。</p>
      </div>
      <div className="space-y-4">
        <HealthStatus overview={overview} />
        <UpdateCheck
          overview={overview}
          update={update}
          onCheckUpdate={onCheckUpdate}
          onPerformUpdate={onPerformUpdate}
        />
      </div>
    </div>
  );
}
