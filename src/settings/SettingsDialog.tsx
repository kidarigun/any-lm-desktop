import { useEffect, useState } from "react";
import { Plug, Server, Settings, Sparkles, X } from "lucide-react";
import { ConnectionDialog } from "../lmstudio/ConnectionDialog";
import { McpDialog } from "../mcp/McpDialog";
import { RuntimePanel } from "../runtime/RuntimePanel";
import { SkillsDialog } from "../skills/SkillsDialog";
import type { AppSettings } from "../types";
import { UpdateSettings, type Updater } from "../update/UpdatePanel";
import { SETTINGS_TABS, type SettingsTab } from "./settingsTabs";

interface Props {
  initialTab?: SettingsTab;
  settings: AppSettings;
  onSaveSettings: (next: AppSettings) => Promise<void>;
  onSettingsChanged: (saved: AppSettings) => void;
  onClose: () => void;
  projectId?: string;
  projectName?: string;
  skillAutoLoad: boolean;
  onSkillAutoLoadChange: (on: boolean) => Promise<void>;
  updater: Updater;
  runningCount: number;
  autoCheckUpdates: boolean;
  onAutoCheckUpdatesChange: (value: boolean) => Promise<void>;
}

const TAB_ICON: Record<SettingsTab, typeof Settings> = {
  connection: Server,
  skills: Sparkles,
  mcp: Plug,
  system: Settings,
};

export function SettingsDialog({
  initialTab = "connection",
  settings,
  onSaveSettings,
  onSettingsChanged,
  onClose,
  projectId,
  projectName,
  skillAutoLoad,
  onSkillAutoLoadChange,
  updater,
  runningCount,
  autoCheckUpdates,
  onAutoCheckUpdatesChange,
}: Props) {
  const [tab, setTab] = useState<SettingsTab>(initialTab);
  const [systemError, setSystemError] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="설정"
        className="bg-white dark:bg-[#262624] border border-stone-200 dark:border-[#3A3A36] rounded-2xl w-full max-w-3xl max-h-[88vh] flex flex-col shadow-xl"
      >
        <div className="shrink-0 px-6 pt-5 pb-3 border-b border-stone-200 dark:border-[#34342F] space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-bold text-stone-900 dark:text-stone-100 flex items-center gap-1.5">
              <Settings className="w-4 h-4 text-[#A50034] dark:text-[#FF5C8D]" />
              설정
            </h3>
            <button onClick={onClose} aria-label="닫기" className="p-1 cursor-pointer">
              <X className="w-4 h-4 text-stone-400" />
            </button>
          </div>

          <div className="flex flex-wrap gap-1.5" role="tablist" aria-label="설정 탭">
            {SETTINGS_TABS.map((item) => {
              const Icon = TAB_ICON[item.id];
              const selected = tab === item.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  role="tab"
                  aria-selected={selected}
                  onClick={() => setTab(item.id)}
                  className={`flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold cursor-pointer transition ${
                    selected
                      ? "bg-stone-800 text-white dark:bg-stone-200 dark:text-stone-900"
                      : "text-stone-600 dark:text-stone-300 hover:bg-stone-100 dark:hover:bg-[#32322F]"
                  }`}
                >
                  <Icon className="w-3.5 h-3.5" />
                  {item.label}
                </button>
              );
            })}
          </div>
        </div>

        <div className="flex-1 min-h-0 overflow-y-auto px-6 py-5">
          {tab === "connection" && (
            <ConnectionDialog
              embedded
              settings={settings}
              onSettingsChanged={onSettingsChanged}
              onSave={onSaveSettings}
              onClose={onClose}
            />
          )}

          {tab === "skills" && (
            <SkillsDialog
              embedded
              projectId={projectId}
              projectName={projectName}
              autoLoad={skillAutoLoad}
              onAutoLoadChange={onSkillAutoLoadChange}
              onClose={onClose}
            />
          )}

          {tab === "mcp" && <McpDialog embedded onClose={onClose} />}

          {tab === "system" && (
            <div className="space-y-5 text-xs">
              <section className="space-y-2">
                <h4 className="font-bold text-stone-900 dark:text-stone-100">실행 환경</h4>
                <RuntimePanel />
              </section>

              <section className="pt-4 border-t border-stone-200 dark:border-[#34342F] space-y-2">
                <h4 className="font-bold text-stone-900 dark:text-stone-100">업데이트</h4>
                <UpdateSettings
                  updater={updater}
                  runningCount={runningCount}
                  autoCheck={autoCheckUpdates}
                  onAutoCheckChange={(value) => {
                    setSystemError(null);
                    void onAutoCheckUpdatesChange(value).catch((err) => setSystemError(String(err)));
                  }}
                />
                {systemError && (
                  <div className="rounded-xl border border-red-200 dark:border-red-900/60 bg-red-50 dark:bg-red-950/40 px-3 py-2 text-xs text-red-700 dark:text-red-300">
                    {systemError}
                  </div>
                )}
              </section>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
