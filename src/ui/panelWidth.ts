/** 문서 캔버스 폭 조절: 기본값·한계값과 저장 */

export const CANVAS_DEFAULT_WIDTH = 520;
export const CANVAS_MIN_WIDTH = 360;
/** 넓게 보기 버튼을 눌렀을 때의 최대 폭 */
export const CANVAS_WIDE_MAX = 1180;
/** 캔버스를 넓혀도 채팅창이 이만큼은 남도록 한다. */
export const CHAT_MIN_WIDTH = 400;
/** 왼쪽 사이드바 폭 (App의 `w-64`) */
export const SIDEBAR_WIDTH = 256;

const STORAGE_KEY = "exaone_canvas_width";

/** 사이드바를 뺀, 채팅창과 캔버스가 나눠 쓰는 폭 */
export function availableWidth(windowWidth: number, sidebarCollapsed = false): number {
  return Math.max(0, windowWidth - (sidebarCollapsed ? 0 : SIDEBAR_WIDTH));
}

/** 채팅창 최소 폭을 남기는 캔버스 최대 폭 (화면이 아주 좁으면 최소 폭) */
export function maxCanvasWidth(available: number): number {
  return Math.max(CANVAS_MIN_WIDTH, available - CHAT_MIN_WIDTH);
}

/** 캔버스 폭을 최소 폭과 최대 폭 사이로 맞춘다. */
export function clampCanvasWidth(width: number, available: number): number {
  return Math.round(Math.min(Math.max(width, CANVAS_MIN_WIDTH), maxCanvasWidth(available)));
}

/** 넓게 보기: 화면의 64%, 최대 1180px */
export function wideCanvasWidth(windowWidth: number, sidebarCollapsed = false): number {
  return clampCanvasWidth(Math.min(windowWidth * 0.64, CANVAS_WIDE_MAX), availableWidth(windowWidth, sidebarCollapsed));
}

export function loadCanvasWidth(): number {
  try {
    const v = Number(localStorage.getItem(STORAGE_KEY));
    return Number.isFinite(v) && v >= CANVAS_MIN_WIDTH ? Math.round(v) : CANVAS_DEFAULT_WIDTH;
  } catch {
    return CANVAS_DEFAULT_WIDTH;
  }
}

export function saveCanvasWidth(width: number) {
  try {
    localStorage.setItem(STORAGE_KEY, String(Math.round(width)));
  } catch {
    // 저장하지 못해도 이번 실행에서는 그대로 동작한다.
  }
}
