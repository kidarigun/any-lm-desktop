import { describe, expect, it } from "vitest";
import { OutputFlow, droppedMarker } from "./outputFlow";

/** 콜백을 직접 부를 때까지 "그리는 중"으로 남는 가짜 xterm */
function fakeTerm() {
  const written: string[] = [];
  const callbacks: (() => void)[] = [];
  return {
    written,
    write: (data: string, done: () => void) => {
      written.push(data);
      callbacks.push(done);
    },
    finishAll() {
      while (callbacks.length) callbacks.shift()!();
    },
  };
}

describe("OutputFlow", () => {
  it("writes straight through while xterm keeps up", () => {
    const t = fakeTerm();
    const flow = new OutputFlow(t.write, 10, 100);
    flow.push("abc");
    t.finishAll();
    flow.push("def");
    expect(t.written).toEqual(["abc", "def"]);
  });

  it("queues while too much is in flight and resumes on the write callback", () => {
    const t = fakeTerm();
    const flow = new OutputFlow(t.write, 4, 100);
    flow.push("12345");
    flow.push("ab");
    flow.push("cd");
    expect(t.written).toEqual(["12345"]);
    expect(flow.pending).toBe(9);
    t.finishAll();
    expect(t.written).toEqual(["12345", "ab", "cd"]);
  });

  it("drops the oldest queued output with the same marker as the backend", () => {
    const t = fakeTerm();
    const flow = new OutputFlow(t.write, 1, 6);
    flow.push("x");
    for (const s of ["aaa", "가나", "ccc", "ddd"]) flow.push(s);
    // 대기열 상한 6글자: "aaa"(3바이트)와 "가나"(6바이트)를 버린다.
    expect(flow.pending).toBeLessThanOrEqual(1 + 6);
    t.finishAll();
    expect(t.written).toEqual(["x", droppedMarker(9), "ccc", "ddd"]);
    expect(droppedMarker(9)).toBe("\x18\x1b[0m\r\n[출력 9바이트 생략]\r\n");
  });

  it("stops writing after dispose even when earlier writes finish", () => {
    const t = fakeTerm();
    const flow = new OutputFlow(t.write, 1, 100);
    flow.push("a");
    flow.push("b");
    flow.dispose();
    t.finishAll();
    flow.push("c");
    expect(t.written).toEqual(["a"]);
    expect(flow.pending).toBe(0);
  });

  it("stays bounded under a flood that xterm never finishes", () => {
    const t = fakeTerm();
    const flow = new OutputFlow(t.write);
    const chunk = "y".repeat(64 * 1024);
    for (let i = 0; i < 400; i++) flow.push(chunk); // 25 MB
    expect(flow.pending).toBeLessThanOrEqual(256 * 1024 + 1024 * 1024 + chunk.length);
  });
});
