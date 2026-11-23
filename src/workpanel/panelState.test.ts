import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { CANVAS_MIN_WIDTH, CHAT_MIN_WIDTH, availableWidth, clampCanvasWidth, maxCanvasWidth } from "../ui/panelWidth";
import {
  NARROW_WINDOW_WIDTH,
  WORK_PANEL_DEFAULT_WIDTH,
  WORK_PANEL_MIN_WIDTH,
  clampWorkPanelWidth,
  loadWorkPanelOpen,
  loadWorkPanelWidth,
  maxWorkPanelWidth,
  saveWorkPanelOpen,
  saveWorkPanelWidth,
  terminalKeyAction,
  visibleSidePanels,
  isWorkPanelShortcut,
  handleWorkPanelShortcut,
  canShowBothPanels,
  canTakeFocus,
  canvasRoom,
  isModalOpen,
} from "./panelState";

const broken = {
  getItem: () => {
    throw new Error("blocked");
  },
  setItem: () => {
    throw new Error("blocked");
  },
};

/** 패널(과 캔버스)을 최대로 넓혔을 때 채팅창에 남는 폭 */
function chatLeft(w: number, collapsed: boolean, panel: number, canvas = 0) {
  return availableWidth(w, collapsed) - panel - canvas;
}

describe("작업 패널 폭", () => {
  it("최소 320px, 최대는 창의 70%이면서 채팅창 400px을 남긴다", () => {
    expect(WORK_PANEL_MIN_WIDTH).toBe(320);
    // 사이드바를 접은 넓은 창: 70%가 먼저 걸린다
    expect(maxWorkPanelWidth(1440, { sidebarCollapsed: true })).toBe(1008);
    // 사이드바(256px)가 열려 있으면 채팅창 400px이 먼저 걸린다: 1440-256-400
    expect(maxWorkPanelWidth(1440)).toBe(784);
    expect(clampWorkPanelWidth(2000, 1440)).toBe(784);
    expect(clampWorkPanelWidth(100, 1440)).toBe(320);
    expect(clampWorkPanelWidth(500.6, 1440)).toBe(501);
    // 캔버스도 함께 보이면 캔버스 최소 폭 360px도 남긴다: 1440-256-400-360
    expect(maxWorkPanelWidth(1440, { canvasShown: true })).toBe(424);
  });

  it("1024px 창: 사이드바 열림 368px, 접힘 624px (채팅창 400px 유지)", () => {
    expect(maxWorkPanelWidth(1024)).toBe(368);
    expect(chatLeft(1024, false, maxWorkPanelWidth(1024))).toBe(CHAT_MIN_WIDTH);
    expect(maxWorkPanelWidth(1024, { sidebarCollapsed: true })).toBe(624);
    expect(chatLeft(1024, true, maxWorkPanelWidth(1024, { sidebarCollapsed: true }))).toBe(CHAT_MIN_WIDTH);
    // 저장된 폭이 넓어도 화면에서는 줄인다
    expect(clampWorkPanelWidth(560, 1024)).toBe(368);
    expect(clampWorkPanelWidth(560, 1024, { sidebarCollapsed: true })).toBe(560);
  });

  it("1280px 창: 사이드바 열림 624px, 접힘+캔버스 520px", () => {
    expect(maxWorkPanelWidth(1280)).toBe(624);
    expect(chatLeft(1280, false, 624)).toBe(CHAT_MIN_WIDTH);
    expect(maxWorkPanelWidth(1280, { sidebarCollapsed: true })).toBe(880);
    expect(chatLeft(1280, true, 880)).toBe(CHAT_MIN_WIDTH);
    const withCanvas = maxWorkPanelWidth(1280, { sidebarCollapsed: true, canvasShown: true });
    expect(withCanvas).toBe(520);
    // 함께 보일 때 캔버스는 패널을 뺀 폭에 맞춘다: 최대 360px, 채팅창 400px
    const room = canvasRoom(1280, true, withCanvas);
    expect(room).toBe(760);
    expect(maxCanvasWidth(room)).toBe(CANVAS_MIN_WIDTH);
    expect(clampCanvasWidth(700, room)).toBe(360);
    expect(chatLeft(1280, true, withCanvas, clampCanvasWidth(700, room))).toBe(CHAT_MIN_WIDTH);
  });

  it("320px로도 채팅창 400px을 못 남기는 좁은 창에서는 채팅창이 좁아진다", () => {
    expect(maxWorkPanelWidth(900)).toBe(320);
    expect(chatLeft(900, false, 320)).toBe(324);
    expect(maxWorkPanelWidth(400)).toBe(320);
    expect(clampWorkPanelWidth(600, 400)).toBe(320);
  });

  it("canvasRoom은 패널이 숨으면 사이드바를 뺀 폭 전부", () => {
    expect(canvasRoom(1280, false, 0)).toBe(1024);
    expect(canvasRoom(1280, true, 0)).toBe(1280);
    expect(canvasRoom(500, false, 320)).toBe(0);
  });
});

/** 테스트 환경에는 localStorage가 없을 수 있어 메모리 저장소를 쓴다. */
function memoryStore() {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v) };
}

describe("작업 패널 기억", () => {
  let store = memoryStore();
  beforeEach(() => {
    store = memoryStore();
  });

  it("폭과 열림 상태를 저장하고 다시 읽는다", () => {
    expect(loadWorkPanelWidth(store)).toBe(WORK_PANEL_DEFAULT_WIDTH);
    expect(loadWorkPanelOpen(store)).toBe(false);
    saveWorkPanelWidth(612.4, store);
    saveWorkPanelOpen(true, store);
    expect(loadWorkPanelWidth(store)).toBe(612);
    expect(loadWorkPanelOpen(store)).toBe(true);
    saveWorkPanelOpen(false, store);
    expect(loadWorkPanelOpen(store)).toBe(false);
  });

  it("잘못 저장된 폭은 기본값으로", () => {
    store.setItem("anylm_workpanel_width", "abc");
    expect(loadWorkPanelWidth(store)).toBe(WORK_PANEL_DEFAULT_WIDTH);
    store.setItem("anylm_workpanel_width", "100");
    expect(loadWorkPanelWidth(store)).toBe(WORK_PANEL_DEFAULT_WIDTH);
    // 저장소가 아예 없어도
    expect(loadWorkPanelWidth(null)).toBe(WORK_PANEL_DEFAULT_WIDTH);
  });

  it("저장소를 쓸 수 없어도 기본값으로 동작한다", () => {
    expect(loadWorkPanelWidth(broken)).toBe(WORK_PANEL_DEFAULT_WIDTH);
    expect(loadWorkPanelOpen(broken)).toBe(false);
    expect(() => saveWorkPanelWidth(500, broken)).not.toThrow();
    expect(() => saveWorkPanelOpen(true, broken)).not.toThrow();
  });
});

describe("캔버스와 작업 패널 함께 보기", () => {
  const both = (windowWidth: number, sidebarCollapsed: boolean, lastOpened: "canvas" | "workpanel" = "canvas") =>
    visibleSidePanels({ canvasOpen: true, panelOpen: true, lastOpened, windowWidth, sidebarCollapsed });

  it("넓은 창에서는 둘 다 보인다", () => {
    expect(both(1440, false)).toEqual({ canvas: true, panel: true });
    expect(both(1440, true)).toEqual({ canvas: true, panel: true });
  });

  it("채팅창·캔버스·패널 최소 폭(400+360+320)이 다 들어갈 때만 함께", () => {
    // 사이드바가 열려 있으면 1280px도 모자란다(1024 < 1080) → 나중에 연 것만
    expect(canShowBothPanels(1280, false)).toBe(false);
    expect(both(1280, false, "workpanel")).toEqual({ canvas: false, panel: true });
    expect(both(1280, false, "canvas")).toEqual({ canvas: true, panel: false });
    // 접으면 1280px에서 함께
    expect(canShowBothPanels(1280, true)).toBe(true);
    expect(both(1280, true)).toEqual({ canvas: true, panel: true });
    // 1024px은 접어도 1100px 미만이라 하나만
    expect(canShowBothPanels(1024, true)).toBe(false);
    expect(canShowBothPanels(1024, false)).toBe(false);
    expect(both(1024, true, "workpanel")).toEqual({ canvas: false, panel: true });
    // 경계: 사이드바 열림은 1336px부터
    expect(canShowBothPanels(1335, false)).toBe(false);
    expect(canShowBothPanels(1336, false)).toBe(true);
  });

  it(`${NARROW_WINDOW_WIDTH}px 미만에서는 사이드바를 접어도 나중에 연 것만 보인다`, () => {
    const w = NARROW_WINDOW_WIDTH - 1;
    expect(both(w, true, "workpanel")).toEqual({ canvas: false, panel: true });
    expect(both(w, true, "canvas")).toEqual({ canvas: true, panel: false });
    expect(both(NARROW_WINDOW_WIDTH, true)).toEqual({ canvas: true, panel: true });
  });

  it("하나만 열려 있으면 좁아도 그것은 보인다", () => {
    expect(visibleSidePanels({ canvasOpen: false, panelOpen: true, lastOpened: "canvas", windowWidth: 900 })).toEqual({
      canvas: false,
      panel: true,
    });
    expect(visibleSidePanels({ canvasOpen: true, panelOpen: false, lastOpened: "workpanel", windowWidth: 900 })).toEqual({
      canvas: true,
      panel: false,
    });
  });
});

const key = (k: Partial<KeyboardEvent>) =>
  ({ ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, key: "", code: "", ...k }) as KeyboardEvent;

describe("Ctrl+` 단축키", () => {
  it("Ctrl+`로 작업 패널을 열고 닫는다", () => {
    expect(isWorkPanelShortcut(key({ ctrlKey: true, key: "`", code: "Backquote" }))).toBe(true);
    // 한글 자판 등 key 값이 달라도 물리 키(Backquote)로 알아본다.
    expect(isWorkPanelShortcut(key({ ctrlKey: true, key: "₩", code: "Backquote" }))).toBe(true);
    expect(isWorkPanelShortcut(key({ ctrlKey: true, key: "`" }))).toBe(true);
  });

  it("다른 조합은 무시한다", () => {
    expect(isWorkPanelShortcut(key({ key: "`", code: "Backquote" }))).toBe(false);
    expect(isWorkPanelShortcut(key({ ctrlKey: true, shiftKey: true, key: "~", code: "Backquote" }))).toBe(false);
    expect(isWorkPanelShortcut(key({ ctrlKey: true, altKey: true, key: "`", code: "Backquote" }))).toBe(false);
    expect(isWorkPanelShortcut(key({ ctrlKey: true, key: "b", code: "KeyB" }))).toBe(false);
  });
});

describe("Ctrl+` 처리기", () => {
  const ev = (k: Partial<KeyboardEvent>) => {
    const calls = { prevented: 0, stopped: 0 };
    const e = {
      ...key(k),
      repeat: false,
      ...k,
      preventDefault: () => void calls.prevented++,
      stopPropagation: () => void calls.stopped++,
    } as unknown as KeyboardEvent;
    return { e, calls };
  };

  it("단축키면 기본 동작을 막고 한 번 토글한다(입력란에 ` 가 들어가지 않게)", () => {
    let toggled = 0;
    const { e, calls } = ev({ ctrlKey: true, key: "`", code: "Backquote" });
    expect(handleWorkPanelShortcut(e, () => toggled++)).toBe(true);
    expect(toggled).toBe(1);
    expect(calls).toEqual({ prevented: 1, stopped: 1 });
  });

  it("누르고 있어 반복되는 키는 토글하지 않는다", () => {
    let toggled = 0;
    const { e, calls } = ev({ ctrlKey: true, key: "`", code: "Backquote", repeat: true });
    expect(handleWorkPanelShortcut(e, () => toggled++)).toBe(true);
    expect(toggled).toBe(0);
    expect(calls.prevented).toBe(1);
  });

  it("한글 조합 중에는 건드리지 않는다", () => {
    let toggled = 0;
    const a = ev({ ctrlKey: true, key: "`", code: "Backquote", isComposing: true });
    expect(handleWorkPanelShortcut(a.e, () => toggled++)).toBe(false);
    const b = ev({ ctrlKey: true, key: "Process", code: "Backquote" });
    expect(handleWorkPanelShortcut(b.e, () => toggled++)).toBe(false);
    expect(toggled).toBe(0);
    expect(a.calls).toEqual({ prevented: 0, stopped: 0 });
  });

  it("대화상자가 떠 있으면 건드리지 않는다", () => {
    let toggled = 0;
    const { e, calls } = ev({ ctrlKey: true, key: "`", code: "Backquote" });
    expect(handleWorkPanelShortcut(e, () => toggled++, () => true)).toBe(false);
    expect(toggled).toBe(0);
    expect(calls).toEqual({ prevented: 0, stopped: 0 });
  });

  it("다른 키는 건드리지 않는다", () => {
    let toggled = 0;
    const { e, calls } = ev({ ctrlKey: true, key: "b", code: "KeyB" });
    expect(handleWorkPanelShortcut(e, () => toggled++)).toBe(false);
    expect(toggled).toBe(0);
    expect(calls).toEqual({ prevented: 0, stopped: 0 });
  });
});

describe("터미널 복사·붙여넣기", () => {
  it("선택이 있으면 Ctrl+C는 복사, 없으면 터미널로(인터럽트)", () => {
    expect(terminalKeyAction(key({ ctrlKey: true, key: "c", code: "KeyC" }), true)).toBe("copy");
    expect(terminalKeyAction(key({ ctrlKey: true, key: "c", code: "KeyC" }), false)).toBe("terminal");
    expect(terminalKeyAction(key({ ctrlKey: true, shiftKey: true, key: "C", code: "KeyC" }), true)).toBe("copy");
  });

  it("Ctrl+V는 붙여넣기, Ctrl+`는 패널 단축키", () => {
    expect(terminalKeyAction(key({ ctrlKey: true, key: "v", code: "KeyV" }), false)).toBe("paste");
    expect(terminalKeyAction(key({ ctrlKey: true, key: "`", code: "Backquote" }), false)).toBe("app");
    expect(terminalKeyAction(key({ key: "a", code: "KeyA" }), true)).toBe("terminal");
  });
});

describe("대화상자와 포커스", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("aria-modal 또는 role=dialog가 있으면 대화상자가 떠 있는 것", () => {
    expect(isModalOpen()).toBe(false);
    document.body.innerHTML = '<div role="dialog"><input id="a"></div>';
    expect(isModalOpen()).toBe(true);
    document.body.innerHTML = '<div aria-modal="true"></div>';
    expect(isModalOpen()).toBe(true);
    document.body.innerHTML = '<div aria-modal="false"></div>';
    expect(isModalOpen()).toBe(false);
  });

  it("포커스가 대화상자 안이면 터미널이 가져가지 않는다", () => {
    document.body.innerHTML = '<div role="dialog"><input id="in"></div><textarea id="out"></textarea>';
    expect(canTakeFocus(document.getElementById("in"))).toBe(false);
    expect(canTakeFocus(document.getElementById("out"))).toBe(true);
    expect(canTakeFocus(document.body)).toBe(true);
    expect(canTakeFocus(null)).toBe(true);
  });
});
