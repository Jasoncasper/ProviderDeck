import { invoke } from "@tauri-apps/api/core";
import { Activity, Home, Info, Zap, type LucideIcon } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { AboutPage } from "@/components/about/AboutPage";
import { LogDiagnosticsTabs } from "@/components/about/LogDiagnosticsTabs";
import { LaunchPage } from "@/components/launch/LaunchPage";
import { Topbar, type TabId } from "@/components/layout/Topbar";
import RoutingPage from "@/components/routing/RoutingPage";
import { NoticeToast } from "@/components/shared/NoticeToast";
import type {
  CommandResult,
  DiagnosticsResult,
  LogsResult,
  OverviewResult,
  RuntimeStatus,
  StartupResult,
  SwitchJournalResult,
  Theme,
  UpdateResult,
} from "@/lib/types";

type LaunchRequest = {
  appPath: string;
  debugPort: number;
  helperPort: number;
};

type TopbarTab = {
  id: TabId;
  label: string;
  icon: LucideIcon;
};

const tabs: TopbarTab[] = [
  { id: "home", label: "首页", icon: Home },
  { id: "routing", label: "Providers", icon: Zap },
  { id: "logdiag", label: "日志诊断", icon: Activity },
  { id: "about", label: "关于", icon: Info },
];

function loadInitialTheme(): Theme {
  const stored = window.localStorage.getItem("providerdeck-theme");
  return stored === "dark" ? "dark" : "light";
}

function buildUpdateRelease(update: UpdateResult | null) {
  if (!update?.latestVersion || !update.assetName || !update.assetUrl) {
    return null;
  }
  return {
    version: update.latestVersion,
    url: "",
    body: update.releaseSummary ?? "",
    asset_name: update.assetName,
    asset_url: update.assetUrl,
  };
}

export function App() {
  const [theme, setTheme] = useState<Theme>(() => loadInitialTheme());
  const [activeTab, setActiveTab] = useState<TabId>("home");
  const [notice, setNotice] = useState<{ title: string; message: string; status?: string } | null>(null);
  const [overview, setOverview] = useState<OverviewResult | null>(null);
  const [update, setUpdate] = useState<UpdateResult | null>(null);
  const [logs, setLogs] = useState<LogsResult | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticsResult | null>(null);
  const [runtime, setRuntime] = useState<RuntimeStatus | null>(null);
  const [journal, setJournal] = useState<SwitchJournalResult | null>(null);

  const showNotice = useCallback((title: string, message: string, status?: string) => {
    setNotice({ title, message, status });
  }, []);

  const run = useCallback(
    async <T,>(task: () => Promise<T>): Promise<T | null> => {
      try {
        return await task();
      } catch (error) {
        showNotice("调用失败", error instanceof Error ? error.message : String(error), "failed");
        return null;
      }
    },
    [showNotice],
  );

  const loadOverview = useCallback(async () => {
    const result = await run(() => invoke<OverviewResult>("load_overview"));
    if (result) {
      setOverview(result);
      if (result.status !== "ok") {
        showNotice("概览", result.message, result.status);
      }
    }
  }, [run, showNotice]);

  const checkUpdate = useCallback(async () => {
    const result = await run(() => invoke<UpdateResult>("check_update"));
    if (result) {
      setUpdate(result);
      if (result.status === "failed") {
        showNotice("GitHub Release 检查", result.message, result.status);
      }
    }
  }, [run, showNotice]);

  const loadLogs = useCallback(async () => {
    const result = await run(() => invoke<LogsResult>("read_latest_logs", { request: { lines: 80 } }));
    if (result) {
      setLogs(result);
      if (result.status !== "ok") {
        showNotice("日志诊断", result.message, result.status);
      }
    }
  }, [run, showNotice]);

  const loadDiagnostics = useCallback(async () => {
    const result = await run(() => invoke<DiagnosticsResult>("copy_diagnostics"));
    if (result) {
      setDiagnostics(result);
      if (result.status !== "ok") {
        showNotice("日志诊断", result.message, result.status);
      }
    }
  }, [run, showNotice]);

  const loadRuntime = useCallback(async () => {
    const [runtimeResult, journalResult] = await Promise.all([
      run(() => invoke<RuntimeStatus>("load_runtime_status")),
      run(() => invoke<SwitchJournalResult>("load_switch_journal")),
    ]);
    if (runtimeResult) setRuntime(runtimeResult);
    if (journalResult) setJournal(journalResult);
  }, [run]);

  const restartProviderDeck = useCallback(async () => {
    const result = await run(() =>
      invoke<CommandResult<Record<string, unknown>>>("restart_providerdeck", {
        request: {
          appPath: "",
          debugPort: 9229,
          helperPort: 57421,
        } satisfies LaunchRequest,
      }),
    );
    if (result) {
      showNotice("重启 ProviderDeck", result.message, result.status);
      await loadOverview();
      await loadRuntime();
    }
  }, [loadOverview, loadRuntime, run, showNotice]);

  const recoverRuntime = useCallback(async () => {
    const result = await run(() => invoke<CommandResult<Record<string, unknown>>>("recover_thread_runtime"));
    if (result) showNotice("Runtime 恢复", result.message, result.status);
    await loadRuntime();
  }, [loadRuntime, run, showNotice]);

  const safeExit = useCallback(async () => {
    await run(() => invoke<CommandResult<Record<string, unknown>>>("safe_exit_providerdeck"));
  }, [run]);

  const performUpdate = useCallback(async () => {
    const release = buildUpdateRelease(update);
    const result = await run(() => invoke<UpdateResult>("perform_update", { release }));
    if (result) {
      setUpdate(result);
      showNotice("更新安装", result.message, result.status);
    }
  }, [run, showNotice, update]);

  const awaitStartup = useCallback(async () => {
    const startup = await run(() => invoke<StartupResult>("startup_options"));
    if (startup?.showUpdate) {
      setActiveTab("about");
    }
    await loadOverview();
    await loadRuntime();
    await checkUpdate();
  }, [checkUpdate, loadOverview, loadRuntime, run]);

  useEffect(() => {
    void awaitStartup();
  }, [awaitStartup]);

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    document.documentElement.classList.toggle("light", theme === "light");
    window.localStorage.setItem("providerdeck-theme", theme);
  }, [theme]);

  useEffect(() => {
    if (activeTab === "logdiag") {
      void loadLogs();
      void loadDiagnostics();
    }
    if (activeTab === "about") {
      void loadOverview();
      void checkUpdate();
    }
  }, [activeTab, checkUpdate, loadDiagnostics, loadLogs, loadOverview]);

  const hasUpdate = update?.updateAvailable === true;

  return (
    <div className={`shell ${theme}`}>
      <Topbar
        activeTab={activeTab}
        tabs={tabs}
        theme={theme}
        hasUpdate={hasUpdate}
        onTabChange={setActiveTab}
        onToggleTheme={() => setTheme((current) => (current === "dark" ? "light" : "dark"))}
      />

      {notice && (
        <NoticeToast
          title={notice.title}
          message={notice.message}
          variant={notice.status === "ok" || notice.status === "not_checked" ? "success" : "error"}
          autoDismiss={notice.status === "ok" || notice.status === "not_checked"}
          onDismiss={() => setNotice(null)}
        />
      )}

      <main className="workspace">
        <section className="flex-1 overflow-auto p-6">
          {activeTab === "home" ? (
            <LaunchPage
              overview={overview}
              runtime={runtime}
              journal={journal}
              onRestart={() => void restartProviderDeck()}
              onRecover={() => void recoverRuntime()}
              onSafeExit={() => void safeExit()}
            />
          ) : null}
          {activeTab === "routing" ? <RoutingPage /> : null}
          {activeTab === "logdiag" ? (
            <LogDiagnosticsTabs
              logs={logs}
              diagnostics={diagnostics}
              onRefreshLogs={() => loadLogs()}
              onCopyLogs={async () => {
                if (!logs) return;
                try {
                  await navigator.clipboard.writeText(logs.text);
                  showNotice("复制日志", "日志已复制。", "ok");
                } catch (error) {
                  showNotice("复制失败", error instanceof Error ? error.message : String(error), "failed");
                }
              }}
              onRefreshDiagnostics={() => loadDiagnostics()}
              onCopyDiagnostics={async () => {
                if (!diagnostics) return;
                try {
                  await navigator.clipboard.writeText(diagnostics.report);
                  showNotice("复制诊断", "诊断报告已复制。", "ok");
                } catch (error) {
                  showNotice("复制失败", error instanceof Error ? error.message : String(error), "failed");
                }
              }}
            />
          ) : null}
          {activeTab === "about" ? (
            <AboutPage
              overview={overview}
              update={update}
              onCheckUpdate={() => void checkUpdate()}
              onPerformUpdate={() => void performUpdate()}
            />
          ) : null}
        </section>
      </main>
    </div>
  );
}
