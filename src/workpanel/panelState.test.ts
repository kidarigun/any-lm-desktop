import { beforeEach, describe, expect, it } from "vitest";
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
} from "./panelState";

const broken = {
  getItem: () => {
    throw new Error("blocked");
  },
  setItem: () => {
    throw new Error("blocked");
  },
};

describe("작업 패널 폭", () => {
  it("최소 320px, 최대 창 너비의 70%", () => {
    expect(WORK_PANEL_MIN_WIDTH).toBe(320);
    expect(maxWorkPanelWidth(1440)).toBe(1008);
    expect(clampWorkPanelWidth(2000, 1440)).toBe(1008);
    expect(clampWorkPanelWidth(100, 1440)).toBe(320);
    expect(clampWorkPanelWidth(500.6, 1440)).toBe(501);
  });

  it("아주 좁은 창에서도 최소 폭은 지킨다", () => {
    expect(maxWorkPanelWidth(400)).toBe(320);
    expect(clampWorkPanelWidth(600, 400)).toBe(320);
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
  it("넓은 창에서는 둘 다 보인다", () => {
    expect(visibleSidePanels({ canvasOpen: true, panelOpen: true, lastOpened: "canvas", windowWidth: 1440 })).toEqual({
      canvas: true,
      panel: true,
    });
  });

  it(`${NARROW_WINDOW_WIDTH}px 미만에서는 나중에 연 것만 보인다`, () => {
    const w = NARROW_WINDOW_WIDTH - 1;
    expect(visibleSidePanels({ canvasOpen: true, panelOpen: true, lastOpened: "workpanel", windowWidth: w })).toEqual({
      canvas: false,
      panel: true,
    });
    expect(visibleSidePanels({ canvasOpen: true, panelOpen: true, lastOpened: "canvas", windowWidth: w })).toEqual({
      canvas: true,
      panel: false,
    });
    expect(
      visibleSidePanels({ canvasOpen: true, panelOpen: true, lastOpened: "canvas", windowWidth: NARROW_WINDOW_WIDTH })
    ).toEqual({ canvas: true, panel: true });
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
