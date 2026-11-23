import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import {
  clampWorkPanelWidth,
  handleWorkPanelShortcut,
  isModalOpen,
  maxWorkPanelWidth,
  loadWorkPanelOpen,
  loadWorkPanelWidth,
  saveWorkPanelOpen,
  saveWorkPanelWidth,
  visibleSidePanels,
  type SidePanel,
} from "./panelState";
import { EMPTY_TABS, tabsReducer, type TabsState } from "./tabs";
import { TerminalAPI } from "./terminalApi";

export interface WorkPanelOptions {
  /** 새 터미널의 작업 폴더 (비우면 사용자 홈) */
  cwd: string;
  /** 문서 캔버스가 열려 있는지 (좁은 창에서 둘 중 나중에 연 것만 보인다) */
  canvasOpen: boolean;
  windowWidth: number;
  /** 왼쪽 사이드바를 접었는지 (패널·캔버스가 쓸 수 있는 폭이 달라진다) */
  sidebarCollapsed: boolean;
}

export interface WorkPanelController {
  open: boolean;
  setOpen: (open: boolean) => void;
  toggle: () => void;
  /** 화면에 쓰는 폭 (창 크기·사이드바·캔버스에 맞춰 줄인 값, 저장된 폭은 그대로) */
  width: number;
  /** 지금 끌어서 넓힐 수 있는 최대 폭 */
  maxWidth: number;
  changeWidth: (width: number) => void;
  tabs: TabsState;
  newTerminal: () => Promise<void>;
  selectTab: (key: string) => void;
  closeTab: (key: string) => Promise<void>;
  restartTab: (key: string) => Promise<void>;
  error: string | null;
  clearError: () => void;
  showCanvas: boolean;
  showPanel: boolean;
}

const DEFAULT_COLS = 100;
const DEFAULT_ROWS = 30;

/**
 * 작업 패널 상태: 열림·폭(기억), 터미널 탭, Ctrl+` 단축키, 셸 종료 알림.
 * 탭 상태는 App에 두므로 패널을 닫아도 세션과 탭은 그대로 남는다.
 */
export function useWorkPanel({ cwd, canvasOpen, windowWidth, sidebarCollapsed }: WorkPanelOptions): WorkPanelController {
  const [open, setOpenState] = useState<boolean>(() => loadWorkPanelOpen());
  const [width, setWidth] = useState<number>(() => loadWorkPanelWidth());
  const [tabs, dispatch] = useReducer(tabsReducer, EMPTY_TABS);
  const [error, setError] = useState<string | null>(null);
  const [lastOpened, setLastOpened] = useState<SidePanel>(() => (loadWorkPanelOpen() ? "workpanel" : "canvas"));
  const [restored, setRestored] = useState<boolean>(() => !TerminalAPI.available());

  const cwdRef = useRef(cwd);
  cwdRef.current = cwd;
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;
  const openRef = useRef(open);
  openRef.current = open;
  const restoredRef = useRef(restored);
  restoredRef.current = restored;
  const opening = useRef(false);

  useEffect(() => saveWorkPanelOpen(open), [open]);

  // 캔버스를 새로 열면 좁은 창에서는 캔버스가 앞에 온다.
  const prevCanvas = useRef(canvasOpen);
  useEffect(() => {
    if (canvasOpen && !prevCanvas.current) setLastOpened("canvas");
    prevCanvas.current = canvasOpen;
  }, [canvasOpen]);

  const newTerminal = useCallback(async () => {
    if (!TerminalAPI.available()) {
      setError("터미널은 데스크톱 앱에서만 쓸 수 있습니다.");
      return;
    }
    if (opening.current) return;
    opening.current = true;
    try {
      const info = await TerminalAPI.open({ cwd: cwdRef.current, cols: DEFAULT_COLS, rows: DEFAULT_ROWS, kind: "shell" });
      dispatch({ type: "open", info });
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      opening.current = false;
    }
  }, []);

  const setOpen = useCallback(
    (next: boolean) => {
      setOpenState(next);
      if (next) {
        setLastOpened("workpanel");
        if (restoredRef.current && tabsRef.current.tabs.length === 0) void newTerminal();
      }
    },
    [newTerminal]
  );

  const toggle = useCallback(() => setOpen(!openRef.current), [setOpen]);
  const toggleRef = useRef(toggle);
  toggleRef.current = toggle;

  // 화면을 다시 불러왔을 때(개발 중 새로고침 등) 살아 있는 세션으로 탭을 복원한다.
  useEffect(() => {
    if (!TerminalAPI.available()) return;
    let cancelled = false;
    TerminalAPI.list()
      .then((sessions) => {
        if (!cancelled) dispatch({ type: "restore", sessions });
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setRestored(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // 패널이 열린 채로 시작했는데 탭이 없으면 터미널 하나를 연다.
  useEffect(() => {
    if (restored && openRef.current && tabsRef.current.tabs.length === 0 && TerminalAPI.available()) void newTerminal();
  }, [restored, newTerminal]);

  // 셸 종료는 패널이 닫혀 있어도 탭에 표시한다.
  useEffect(() => {
    let off: (() => void) | null = null;
    let cancelled = false;
    void TerminalAPI.onExit((p) => dispatch({ type: "exit", sessionId: p.id, code: p.code })).then((f) => {
      if (cancelled) f();
      else off = f;
    });
    return () => {
      cancelled = true;
      off?.();
    };
  }, []);

  // Ctrl+` : 채팅 입력란·터미널 안에서도 먼저 받는다(capture).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      handleWorkPanelShortcut(e, () => toggleRef.current(), () => isModalOpen());
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const selectTab = useCallback((key: string) => dispatch({ type: "select", key }), []);

  const closeTab = useCallback(async (key: string) => {
    const tab = tabsRef.current.tabs.find((t) => t.key === key);
    if (!tab) return;
    if (tab.kind === "terminal") {
      if (!tab.exited) {
        const busy = await TerminalAPI.busy(tab.sessionId).catch(() => false);
        if (busy && !window.confirm("이 터미널에서 실행 중인 프로그램이 있습니다. 탭을 닫으면 함께 종료됩니다. 닫을까요?")) {
          return;
        }
      }
      await TerminalAPI.close(tab.sessionId).catch(() => {});
    }
    dispatch({ type: "close", key });
  }, []);

  const restartTab = useCallback(async (key: string) => {
    const tab = tabsRef.current.tabs.find((t) => t.key === key);
    if (!tab || tab.kind !== "terminal") return;
    await TerminalAPI.close(tab.sessionId).catch(() => {});
    try {
      const info = await TerminalAPI.open({
        cwd: tab.cwd,
        cols: DEFAULT_COLS,
        rows: DEFAULT_ROWS,
        kind: tab.terminalKind,
        command: tab.command,
      });
      dispatch({ type: "restart", key, info });
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const visible = visibleSidePanels({ canvasOpen, panelOpen: open, lastOpened, windowWidth, sidebarCollapsed });
  // 창 크기·사이드바·캔버스가 바뀔 때마다 다시 맞춘다(저장된 폭은 그대로 두고 화면에 쓰는 폭만).
  const room = { sidebarCollapsed, canvasShown: visible.canvas };
  const roomRef = useRef(room);
  roomRef.current = room;

  const changeWidth = useCallback((w: number) => {
    const c = clampWorkPanelWidth(w, window.innerWidth, roomRef.current);
    setWidth(c);
    saveWorkPanelWidth(c);
  }, []);

  return {
    open,
    setOpen,
    toggle,
    width: clampWorkPanelWidth(width, windowWidth, room),
    maxWidth: maxWorkPanelWidth(windowWidth, room),
    changeWidth,
    tabs,
    newTerminal,
    selectTab,
    closeTab,
    restartTab,
    error,
    clearError: () => setError(null),
    showCanvas: visible.canvas,
    showPanel: visible.panel,
  };
}
