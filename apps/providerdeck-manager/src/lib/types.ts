// ============================================================
// ProviderDeck shared types
// ============================================================

export type Status = "ok" | "failed" | "not_implemented" | "not_checked" | string;

export type CommandResult<T> = T & {
  status: Status;
  message: string;
};

export type PathState = {
  status: string;
  path: string | null;
};

export type LaunchStatus = {
  status: string;
  message: string;
  started_at_ms: number;
  debug_port: number | null;
  helper_port: number | null;
  codex_app: string | null;
};

export type OverviewResult = CommandResult<{
  codex_app: PathState;
  codex_version: string | null;
  silent_shortcut: PathState;
  management_shortcut: PathState;
  latest_launch: LaunchStatus | null;
  current_version: string;
  update_status: string;
  settings_path: string;
  logs_path: string;
}>;

export type LogsResult = CommandResult<{
  path: string;
  text: string;
  lines: number;
}>;

export type DiagnosticsResult = CommandResult<{
  report: string;
}>;

export type UpdateResult = CommandResult<{
  currentVersion: string;
  latestVersion?: string | null;
  releaseSummary?: string;
  assetName?: string | null;
  assetUrl?: string | null;
  updateAvailable?: boolean;
  installedPath?: string;
  progress?: number;
}>;

export type StartupResult = CommandResult<{
  showUpdate: boolean;
}>;

export type RuntimeStatus = CommandResult<{
  appServerConnected: boolean;
  bridgeInjected: boolean;
  helperHealthy: boolean;
  helperPort: number;
  connectedThreads: number;
  switchPhase: string;
}>;

export type SwitchJournalResult = CommandResult<{
  record: {
    threadId?: string;
    phase?: string;
    target?: { model?: string; providerId?: string };
    error?: string | null;
    updatedAtMs?: number;
  } | null;
}>;

export type Theme = "dark" | "light";
