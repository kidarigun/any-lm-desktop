import { describe, expect, it } from "vitest";
import { EMPTY_TABS, activeTab, tabsReducer, terminalTabs, type TabsState } from "./tabs";
import type { TerminalInfo } from "./terminalApi";

const info = (id: string, extra: Partial<TerminalInfo> = {}): TerminalInfo => ({
  id,
  kind: "shell",
  title: "PowerShell",
  cwd: "C:\\work",
  command: null,
  shell: "powershell",
  pid: 1,
  running: true,
  exitCode: null,
  ...extra,
});

const openAll = (...ids: string[]): TabsState => ids.reduce((s, id) => tabsReducer(s, { type: "open", info: info(id) }), EMPTY_TABS);

describe("작업 패널 탭", () => {
  it("터미널을 열면 탭이 생기고 그 탭이 선택된다. 같은 이름에는 번호를 붙인다", () => {
    const s = openAll("a", "b", "c");
    expect(s.tabs.map((t) => t.title)).toEqual(["PowerShell", "PowerShell 2", "PowerShell 3"]);
    const active = activeTab(s);
    expect(active?.kind === "terminal" ? active.sessionId : null).toBe("c");
    expect(terminalTabs(s).every((t) => t.terminalKind === "shell" && !t.exited)).toBe(true);
  });

  it("탭을 고르고, 닫으면 오른쪽(없으면 왼쪽) 탭이 선택된다", () => {
    let s = openAll("a", "b", "c");
    const [ka, kb, kc] = s.tabs.map((t) => t.key);
    s = tabsReducer(s, { type: "select", key: kb });
    expect(s.activeKey).toBe(kb);
    s = tabsReducer(s, { type: "close", key: kb });
    expect(s.tabs.map((t) => t.key)).toEqual([ka, kc]);
    expect(s.activeKey).toBe(kc);
    s = tabsReducer(s, { type: "close", key: kc });
    expect(s.activeKey).toBe(ka);
    s = tabsReducer(s, { type: "close", key: ka });
    expect(s).toEqual(EMPTY_TABS);
    // 없는 탭은 무시
    expect(tabsReducer(s, { type: "select", key: "nope" })).toBe(s);
  });

  it("선택하지 않은 탭을 닫으면 선택은 그대로", () => {
    let s = openAll("a", "b");
    const [ka, kb] = s.tabs.map((t) => t.key);
    s = tabsReducer(s, { type: "close", key: ka });
    expect(s.activeKey).toBe(kb);
  });

  it("셸이 끝나면 탭에 종료 코드를 표시하고, 다시 시작하면 같은 자리에서 새 세션으로 바뀐다", () => {
    let s = openAll("a", "b");
    const ka = s.tabs[0].key;
    s = tabsReducer(s, { type: "exit", sessionId: "a", code: 1 });
    const t = terminalTabs(s)[0];
    expect(t.exited).toBe(true);
    expect(t.exitCode).toBe(1);
    // 모르는 세션의 종료는 무시
    expect(tabsReducer(s, { type: "exit", sessionId: "zzz", code: 0 })).toBe(s);
    s = tabsReducer(s, { type: "restart", key: ka, info: info("a2") });
    const r = terminalTabs(s)[0];
    expect(r.key).toBe(ka);
    expect(r.sessionId).toBe("a2");
    expect(r.exited).toBe(false);
    expect(r.exitCode).toBeNull();
    expect(r.title).toBe("PowerShell");
    expect(s.activeKey).toBe(ka);
  });

  it("앱 화면을 다시 불러오면 살아 있는 세션으로 탭을 복원한다(이미 있는 탭은 그대로)", () => {
    let s = openAll("a");
    s = tabsReducer(s, {
      type: "restore",
      sessions: [info("a"), info("b", { running: false, exitCode: 2, kind: "server", title: "npm run dev", command: "npm run dev" })],
    });
    const tabs = terminalTabs(s);
    expect(tabs.map((t) => t.sessionId)).toEqual(["a", "b"]);
    expect(tabs[1]).toMatchObject({ exited: true, exitCode: 2, terminalKind: "server", title: "npm run dev", command: "npm run dev" });
    expect(activeTab(s)?.key).toBe(tabs[0].key);
    // 빈 상태에서 복원하면 첫 탭을 고른다.
    const fresh = tabsReducer(EMPTY_TABS, { type: "restore", sessions: [info("x"), info("y")] });
    expect(fresh.activeKey).toBe(fresh.tabs[0].key);
    expect(fresh.tabs.map((t) => t.title)).toEqual(["PowerShell", "PowerShell 2"]);
    expect(tabsReducer(EMPTY_TABS, { type: "restore", sessions: [] })).toEqual(EMPTY_TABS);
  });
});
