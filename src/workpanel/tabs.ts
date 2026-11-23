/**
 * 작업 패널 탭 모델.
 * 지금은 터미널(셸) 탭만 쓰고, T2에서 서버 터미널(`terminalKind: "server"`)과 미리보기 탭(`kind: "preview"`)을 더한다.
 */
import type { TerminalInfo, TerminalKind } from "./terminalApi";

export interface TerminalTab {
  kind: "terminal";
  /** 탭 자리 (다시 시작해도 그대로) */
  key: string;
  /** 지금 연결된 백엔드 세션 */
  sessionId: string;
  title: string;
  terminalKind: TerminalKind;
  cwd: string;
  command: string | null;
  exited: boolean;
  exitCode: number | null;
}

/** T2: 앱 안 미리보기 (아직 만들지 않음) */
export interface PreviewTab {
  kind: "preview";
  key: string;
  url: string;
  title: string;
}

export type WorkTab = TerminalTab | PreviewTab;

export interface TabsState {
  tabs: WorkTab[];
  activeKey: string | null;
}

export const EMPTY_TABS: TabsState = { tabs: [], activeKey: null };

export type TabsAction =
  | { type: "open"; info: TerminalInfo }
  | { type: "close"; key: string }
  | { type: "select"; key: string }
  | { type: "exit"; sessionId: string; code: number | null }
  | { type: "restart"; key: string; info: TerminalInfo }
  | { type: "restore"; sessions: TerminalInfo[] };

/** 같은 이름의 탭이 있으면 번호를 붙인다: PowerShell, PowerShell 2, … */
function numberedTitle(tabs: WorkTab[], base: string): string {
  const taken = new Set(tabs.map((t) => t.title));
  if (!taken.has(base)) return base;
  for (let n = 2; ; n++) {
    const t = `${base} ${n}`;
    if (!taken.has(t)) return t;
  }
}

function tabFromInfo(tabs: WorkTab[], info: TerminalInfo): TerminalTab {
  return {
    kind: "terminal",
    key: `tab-${info.id}`,
    sessionId: info.id,
    title: numberedTitle(tabs, info.title),
    terminalKind: info.kind,
    cwd: info.cwd,
    command: info.command ?? null,
    exited: !info.running,
    exitCode: info.running ? null : info.exitCode ?? null,
  };
}

export function tabsReducer(state: TabsState, action: TabsAction): TabsState {
  switch (action.type) {
    case "open": {
      const tab = tabFromInfo(state.tabs, action.info);
      return { tabs: [...state.tabs, tab], activeKey: tab.key };
    }
    case "close": {
      const i = state.tabs.findIndex((t) => t.key === action.key);
      if (i < 0) return state;
      const tabs = state.tabs.filter((t) => t.key !== action.key);
      let activeKey = state.activeKey;
      if (activeKey === action.key) activeKey = (tabs[i] ?? tabs[i - 1])?.key ?? null;
      return { tabs, activeKey };
    }
    case "select":
      return state.tabs.some((t) => t.key === action.key) && state.activeKey !== action.key
        ? { ...state, activeKey: action.key }
        : state;
    case "exit": {
      if (!state.tabs.some((t) => t.kind === "terminal" && t.sessionId === action.sessionId)) return state;
      return {
        ...state,
        tabs: state.tabs.map((t) =>
          t.kind === "terminal" && t.sessionId === action.sessionId ? { ...t, exited: true, exitCode: action.code } : t
        ),
      };
    }
    case "restart":
      return {
        tabs: state.tabs.map((t) =>
          t.kind === "terminal" && t.key === action.key
            ? { ...t, sessionId: action.info.id, cwd: action.info.cwd, exited: false, exitCode: null }
            : t
        ),
        activeKey: action.key,
      };
    case "restore": {
      const known = new Set(state.tabs.flatMap((t) => (t.kind === "terminal" ? [t.sessionId] : [])));
      let tabs = state.tabs;
      for (const info of action.sessions) {
        if (known.has(info.id)) continue;
        tabs = [...tabs, tabFromInfo(tabs, info)];
      }
      if (tabs === state.tabs) return state;
      return { tabs, activeKey: state.activeKey ?? tabs[0]?.key ?? null };
    }
  }
}

export function activeTab(state: TabsState): WorkTab | undefined {
  return state.tabs.find((t) => t.key === state.activeKey);
}

export function terminalTabs(state: TabsState): TerminalTab[] {
  return state.tabs.filter((t): t is TerminalTab => t.kind === "terminal");
}
