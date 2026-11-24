import { useEffect, useRef, useState } from "react";
import { Globe, PanelRightClose, Plus, RotateCcw, SquareTerminal, X } from "lucide-react";
import { PanelResizer } from "../ui/PanelResizer";
import { WORK_PANEL_DEFAULT_WIDTH, WORK_PANEL_MIN_WIDTH } from "./panelState";
import { TerminalView } from "./TerminalView";
import { PreviewView } from "./PreviewView";
import { ServerMenu } from "./ServerMenu";
import { TerminalAPI, type ShellOption } from "./terminalApi";
import type { WorkPanelController } from "./useWorkPanel";

export interface WorkPanelProps {
  panel: WorkPanelController;
  isDark: boolean;
  windowWidth: number;
  /** 설정의 셸 ("powershell" | "pwsh" | "cmd") */
  shell: string;
  onShellChange: (id: string) => void;
  /** 터미널 출력 속 링크 */
  onLink: (url: string) => void;
}

/** 오른쪽 작업 패널 */
export function WorkPanel({ panel, isDark, windowWidth, shell, onShellChange, onLink }: WorkPanelProps) {
  const ref = useRef<HTMLElement>(null);
  const [shells, setShells] = useState<ShellOption[]>([]);
  useEffect(() => {
    let cancelled = false;
    TerminalAPI.shells()
      .then((s) => !cancelled && setShells(s))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const { tabs } = panel;
  const termBg = isDark ? "bg-[#1A1A18]" : "bg-white";

  return (
    <aside
      ref={ref}
      style={{ width: panel.width }}
      aria-label="작업 패널"
      className="relative bg-white dark:bg-[#232321] border-l border-[#E2E0D8] dark:border-[#2E2E2A] flex flex-col shrink-0 min-w-0 transition-[width] duration-150"
    >
      <PanelResizer
        target={ref}
        width={panel.width}
        min={WORK_PANEL_MIN_WIDTH}
        max={panel.maxWidth}
        onChange={panel.changeWidth}
        onReset={() => panel.changeWidth(WORK_PANEL_DEFAULT_WIDTH)}
        label="작업 패널 폭 조절"
      />
      <div className="h-10 shrink-0 flex items-center gap-1 px-1.5 border-b border-[#E5E3DB] dark:border-[#2E2E2A]">
        <div role="tablist" aria-label="작업 패널 탭" className="flex items-center gap-0.5 min-w-0 overflow-x-auto"
          style={{ scrollbarWidth: "none" }}
        >
          {tabs.tabs.map((t) => {
            const selected = t.key === tabs.activeKey;
            const exited = t.kind === "terminal" && t.exited;
            return (
              <div
                key={t.key}
                role="tab"
                aria-selected={selected}
                tabIndex={0}
                onClick={() => panel.selectTab(t.key)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    panel.selectTab(t.key);
                  }
                }}
                onAuxClick={(e) => {
                  if (e.button === 1) void panel.closeTab(t.key);
                }}
                title={t.kind === "terminal" ? t.cwd : t.url}
                className={`group flex items-center gap-1.5 pl-2 pr-1 h-7 rounded-md text-xs cursor-pointer select-none shrink-0 max-w-[180px] ${
                  selected
                    ? "bg-stone-200/80 dark:bg-[#32322F] text-stone-900 dark:text-stone-100 font-semibold"
                    : "text-stone-600 dark:text-stone-400 hover:bg-stone-100 dark:hover:bg-[#2A2A27]"
                }`}
              >
                {t.kind === "preview"
                  ? <Globe className="w-3.5 h-3.5 shrink-0" />
                  : <SquareTerminal className={`w-3.5 h-3.5 shrink-0 ${exited ? "opacity-40" : ""}`} />}
                <span className={`truncate ${exited ? "opacity-60" : ""}`}>{t.title}</span>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    void panel.closeTab(t.key);
                  }}
                  aria-label={`${t.title} 닫기`}
                  title="닫기"
                  className="p-0.5 rounded hover:bg-stone-300/70 dark:hover:bg-[#3A3A36] opacity-60 group-hover:opacity-100 cursor-pointer"
                >
                  <X className="w-3 h-3" />
                </button>
              </div>
            );
          })}
        </div>
        <button
          type="button"
          onClick={() => void panel.newTerminal()}
          title="새 터미널"
          className="flex items-center gap-1 px-2 h-7 rounded-md text-xs font-semibold text-stone-700 dark:text-stone-200 hover:bg-stone-100 dark:hover:bg-[#2A2A27] cursor-pointer whitespace-nowrap shrink-0"
        >
          <Plus className="w-3.5 h-3.5" />
          <span className="hidden min-[1250px]:inline">새 터미널</span>
        </button>
        <ServerMenu key={panel.projectId} panel={panel} />
        <div className="flex-1" />
        {shells.length > 0 && (
          <select
            value={shells.some((s) => s.id === shell) ? shell : "powershell"}
            onChange={(e) => onShellChange(e.target.value)}
            title="새 터미널에 쓸 셸 (새로 여는 터미널부터 적용)"
            aria-label="셸"
            className="h-7 max-w-[130px] rounded-md border border-stone-300 dark:border-[#3A3A36] bg-white dark:bg-[#262624] text-xs text-stone-700 dark:text-stone-200 px-1 cursor-pointer shrink-0"
          >
            {shells.map((s) => (
              <option key={s.id} value={s.id}>
                {s.label}
              </option>
            ))}
          </select>
        )}
        <button
          type="button"
          onClick={() => panel.setOpen(false)}
          title="작업 패널 닫기 (Ctrl+`) — 터미널은 계속 실행됩니다"
          aria-label="작업 패널 닫기"
          className="p-1.5 rounded-md text-stone-600 dark:text-stone-300 hover:bg-stone-100 dark:hover:bg-[#2A2A27] cursor-pointer shrink-0"
        >
          <PanelRightClose className="w-4 h-4" />
        </button>
      </div>

      {panel.error && (
        <div className="shrink-0 bg-red-50 dark:bg-red-950/60 border-b border-red-200 dark:border-red-900 px-3 py-1.5 text-xs text-red-700 dark:text-red-300 flex items-start justify-between gap-2">
          <span className="min-w-0 break-words">{panel.error}</span>
          <button type="button" onClick={panel.clearError} aria-label="닫기" className="shrink-0 cursor-pointer">
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      )}

      <div className={`relative flex-1 min-h-0 ${termBg}`}>
        {tabs.tabs.length === 0 && (
          <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 text-xs text-stone-500 dark:text-stone-400">
            <span>열린 터미널이 없습니다.</span>
            <button
              type="button"
              onClick={() => void panel.newTerminal()}
              className="flex items-center gap-1 px-3 py-1.5 rounded-lg border border-stone-300 dark:border-[#3A3A36] font-semibold text-stone-700 dark:text-stone-200 hover:bg-stone-100 dark:hover:bg-[#2A2A27] cursor-pointer"
            >
              <Plus className="w-3.5 h-3.5" /> 새 터미널
            </button>
          </div>
        )}
        {tabs.tabs.map((t) => {
          const selected = t.key === tabs.activeKey;
          if (t.kind === "preview") {
            return (
              <div key={t.key} className={selected ? "absolute inset-0" : "hidden"}>
                <PreviewView initialUrl={t.url} />
              </div>
            );
          }
          const server = panel.servers[t.sessionId];
          return (
            <div key={t.sessionId} className={selected ? "absolute inset-0 flex flex-col" : "hidden"}>
              {server && !t.exited && (
                <div className="flex shrink-0 items-center gap-2 border-b border-stone-200 px-2 py-1 text-xs dark:border-stone-700">
                  <span role="status" className="shrink-0">
                    {server.status === "ready" ? "● 실행 중" : server.status === "starting" ? "◌ 시작 중…" : "◌ 주소 연결 미확인"}
                  </span>
                  {server.url && (
                    <button type="button" title={server.url} className="min-w-0 truncate text-blue-600 dark:text-blue-300"
                      onClick={() => panel.openPreview(server.url!)}>{server.url}</button>
                  )}
                </div>
              )}
              <div className="flex-1 min-h-0">
                <TerminalView sessionId={t.sessionId} active={selected} isDark={isDark} onLink={onLink} />
              </div>
              {t.exited && (
                <div className="shrink-0 flex items-center gap-3 px-3 py-1.5 border-t border-[#E5E3DB] dark:border-[#2E2E2A] bg-stone-50 dark:bg-[#232321] text-xs text-stone-600 dark:text-stone-300">
                  <span>종료됨 (코드 {t.exitCode ?? "알 수 없음"})</span>
                  <button
                    type="button"
                    onClick={() => void panel.restartTab(t.key)}
                    className="flex items-center gap-1 px-2 py-1 rounded-md border border-stone-300 dark:border-[#3A3A36] font-semibold hover:bg-stone-100 dark:hover:bg-[#2A2A27] cursor-pointer"
                  >
                    <RotateCcw className="w-3 h-3" /> 다시 시작
                  </button>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </aside>
  );
}

export default WorkPanel;
