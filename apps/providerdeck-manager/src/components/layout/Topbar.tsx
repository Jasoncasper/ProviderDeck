import { useState } from "react";
import { Moon, Sun, type LucideIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ContactDialog } from "@/components/shared/ContactDialog";
import type { Theme } from "@/lib/types";

export type TabId = "home" | "routing" | "logdiag" | "about";

export interface TabDef {
  id: TabId;
  label: string;
  icon: LucideIcon;
}

interface TopbarProps {
  activeTab: TabId;
  tabs: TabDef[];
  theme: Theme;
  hasUpdate: boolean;
  onTabChange: (tab: TabId) => void;
  onToggleTheme: () => void;
}

export function Topbar({
  activeTab,
  tabs,
  theme,
  hasUpdate,
  onTabChange,
  onToggleTheme,
}: TopbarProps) {
  const [contactOpen, setContactOpen] = useState(false);

  return (
    <>
      <header className="topbar-horizontal" data-tauri-drag-region>
        <div className="topbar-left macos-drag-pad">
          <div className="topbar-brand">
            <img className="topbar-brand-icon" src="/app-icon.png" alt="" />
            <span className="topbar-brand-text">ProviderDeck</span>
            {hasUpdate && (
              <span className="topbar-update-badge" title="有新版本可用">有更新</span>
            )}
          </div>
        </div>

        <nav className="topbar-tabs">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const active = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                className={`topbar-tab ${active ? "active" : ""}`}
                onClick={() => onTabChange(tab.id)}
                type="button"
              >
                <Icon className="h-4 w-4" />
                <span>{tab.label}</span>
              </button>
            );
          })}
        </nav>

        <div className="topbar-actions">
          <Button
            onClick={() => setContactOpen(true)}
            title="问题反馈"
            variant="outline"
          >
            <svg className="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
              <circle cx="9" cy="7" r="4" />
              <path d="M22 21v-2a4 4 0 0 0-3-3.87" />
              <path d="M16 3.13a4 4 0 0 1 0 7.75" />
            </svg>
            <span>问题反馈</span>
          </Button>
          <Button
            onClick={onToggleTheme}
            title={theme === "dark" ? "切换到浅色" : "切换到深色"}
            variant="outline"
            size="icon"
          >
            {theme === "dark" ? (
              <Sun className="h-4 w-4" />
            ) : (
              <Moon className="h-4 w-4" />
            )}
          </Button>
        </div>
      </header>
      <ContactDialog open={contactOpen} onClose={() => setContactOpen(false)} />
    </>
  );
}
