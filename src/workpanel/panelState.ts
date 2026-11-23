/** 오른쪽 작업 패널: 폭·열림 상태 기억, 캔버스와 함께 보기, 단축키 규칙 (결정 0008) */
import { CANVAS_MIN_WIDTH, CHAT_MIN_WIDTH, availableWidth } from "../ui/panelWidth";

export const WORK_PANEL_MIN_WIDTH = 320;
/** 창 너비 대비 최대 폭 */
export const WORK_PANEL_MAX_RATIO = 0.7;
export const WORK_PANEL_DEFAULT_WIDTH = 560;
/** 이보다 좁은 창에서는 캔버스와 작업 패널 중 나중에 연 것만 보인다. */
export const NARROW_WINDOW_WIDTH = 1100;

const WIDTH_KEY = "anylm_workpanel_width";
const OPEN_KEY = "anylm_workpanel_open";

type KeyValueStore = Pick<Storage, "getItem" | "setItem">;

function defaultStore(): KeyValueStore | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

/** 패널 폭을 정할 때 함께 따지는 화면 상태 */
export interface PanelRoom {
  sidebarCollapsed?: boolean;
  /** 문서 캔버스도 함께 보이는지 (그러면 캔버스 최소 폭도 남긴다) */
  canvasShown?: boolean;
}

/**
 * 작업 패널 최대 폭: 창의 70%이면서 채팅창 최소 폭(400px)과, 캔버스가 함께 보이면 캔버스 최소 폭(360px)을 남긴다.
 * 그래도 320px이 안 되는 아주 좁은 창에서는 320px을 지키고 채팅창이 좁아지는 것을 받아들인다.
 */
export function maxWorkPanelWidth(windowWidth: number, room: PanelRoom = {}): number {
  const ratio = Math.floor((windowWidth * Math.round(WORK_PANEL_MAX_RATIO * 100)) / 100);
  const left =
    availableWidth(windowWidth, !!room.sidebarCollapsed) - CHAT_MIN_WIDTH - (room.canvasShown ? CANVAS_MIN_WIDTH : 0);
  return Math.max(WORK_PANEL_MIN_WIDTH, Math.min(ratio, left));
}

export function clampWorkPanelWidth(width: number, windowWidth: number, room: PanelRoom = {}): number {
  return Math.round(Math.min(Math.max(width, WORK_PANEL_MIN_WIDTH), maxWorkPanelWidth(windowWidth, room)));
}

/** 작업 패널이 보일 때 캔버스와 채팅창이 나눠 쓰는 폭 */
export function canvasRoom(windowWidth: number, sidebarCollapsed: boolean, panelShownWidth: number): number {
  return Math.max(0, availableWidth(windowWidth, sidebarCollapsed) - panelShownWidth);
}

/** 캔버스와 작업 패널을 함께 보여도 되는 창인지: 1100px 이상이고, 채팅창·캔버스·패널 최소 폭이 다 들어갈 때 */
export function canShowBothPanels(windowWidth: number, sidebarCollapsed = false): boolean {
  return (
    windowWidth >= NARROW_WINDOW_WIDTH &&
    availableWidth(windowWidth, sidebarCollapsed) >= CHAT_MIN_WIDTH + CANVAS_MIN_WIDTH + WORK_PANEL_MIN_WIDTH
  );
}

export function loadWorkPanelWidth(store: KeyValueStore | null = defaultStore()): number {
  try {
    const raw = store?.getItem(WIDTH_KEY);
    const v = raw == null ? NaN : Number(raw);
    return Number.isFinite(v) && v >= WORK_PANEL_MIN_WIDTH ? Math.round(v) : WORK_PANEL_DEFAULT_WIDTH;
  } catch {
    return WORK_PANEL_DEFAULT_WIDTH;
  }
}

export function saveWorkPanelWidth(width: number, store: KeyValueStore | null = defaultStore()) {
  try {
    store?.setItem(WIDTH_KEY, String(Math.round(width)));
  } catch {
    // 저장하지 못해도 이번 실행에서는 그대로 동작한다.
  }
}

export function loadWorkPanelOpen(store: KeyValueStore | null = defaultStore()): boolean {
  try {
    return store?.getItem(OPEN_KEY) === "true";
  } catch {
    return false;
  }
}

export function saveWorkPanelOpen(open: boolean, store: KeyValueStore | null = defaultStore()) {
  try {
    store?.setItem(OPEN_KEY, open ? "true" : "false");
  } catch {
    // 저장하지 못해도 이번 실행에서는 그대로 동작한다.
  }
}

export type SidePanel = "canvas" | "workpanel";

/** 문서 캔버스와 작업 패널 가운데 화면에 보일 것. 둘 다 들어가지 않는 창에서는 나중에 연 것만. */
export function visibleSidePanels(p: {
  canvasOpen: boolean;
  panelOpen: boolean;
  lastOpened: SidePanel;
  windowWidth: number;
  sidebarCollapsed?: boolean;
}): { canvas: boolean; panel: boolean } {
  if (p.canvasOpen && p.panelOpen && !canShowBothPanels(p.windowWidth, !!p.sidebarCollapsed)) {
    return { canvas: p.lastOpened === "canvas", panel: p.lastOpened === "workpanel" };
  }
  return { canvas: p.canvasOpen, panel: p.panelOpen };
}

type KeyLike = Pick<KeyboardEvent, "ctrlKey" | "metaKey" | "shiftKey" | "altKey" | "key" | "code">;

/** Ctrl+` (자판 배열과 상관없이 물리 키 Backquote도 받는다) */
export function isWorkPanelShortcut(e: KeyLike): boolean {
  if (!e.ctrlKey || e.shiftKey || e.altKey || e.metaKey) return false;
  return e.code === "Backquote" || e.key === "`";
}

/**
 * 창 전체 keydown(capture) 처리: Ctrl+`이면 기본 동작을 막고 패널을 열고 닫는다(누르고 있어 반복되면 한 번만).
 * 처리했으면 true.
 */
export function handleWorkPanelShortcut(
  e: KeyLike & Pick<KeyboardEvent, "repeat" | "preventDefault" | "stopPropagation" | "isComposing">,
  toggle: () => void,
  modalOpen: () => boolean = () => false
): boolean {
  // 한글 조합 중이거나 대화상자가 떠 있으면 건드리지 않는다.
  if (e.isComposing || e.key === "Process" || !isWorkPanelShortcut(e) || modalOpen()) return false;
  e.preventDefault();
  e.stopPropagation();
  if (!e.repeat) toggle();
  return true;
}

/**
 * 터미널 안에서 누른 키를 누가 처리할지.
 * - copy: 선택한 글자를 복사(Ctrl+C·Ctrl+Shift+C, 선택이 있을 때)
 * - paste: 브라우저 붙여넣기에 맡긴다(Ctrl+V·Ctrl+Shift+V)
 * - app: 앱 단축키(Ctrl+`)
 * - terminal: 터미널로 보낸다(선택 없는 Ctrl+C는 인터럽트)
 */
export function terminalKeyAction(e: KeyLike, hasSelection: boolean): "copy" | "paste" | "app" | "terminal" {
  if (isWorkPanelShortcut(e)) return "app";
  if (e.ctrlKey && !e.altKey && !e.metaKey) {
    const k = e.code === "KeyC" || e.key.toLowerCase() === "c" ? "c" : e.code === "KeyV" || e.key.toLowerCase() === "v" ? "v" : "";
    if (k === "c" && hasSelection) return "copy";
    if (k === "v") return "paste";
  }
  return "terminal";
}

/** 대화상자(모달)를 나타내는 요소 */
export const MODAL_SELECTOR = '[aria-modal="true"], [role="dialog"]';

/** 화면에 대화상자가 떠 있는지 */
export function isModalOpen(doc: Pick<Document, "querySelector"> = document): boolean {
  return !!doc.querySelector(MODAL_SELECTOR);
}

/** 터미널이 포커스를 가져가도 되는지: 지금 포커스가 대화상자 안이면 안 된다. */
export function canTakeFocus(active: Element | null): boolean {
  return !active?.closest?.(MODAL_SELECTOR);
}
