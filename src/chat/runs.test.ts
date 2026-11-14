import { describe, expect, it } from "vitest";
import type { StreamChunkPayload, ToolExecutionRecord } from "../types";
import { applyChunk, countByMode, countRunning, endRun, startRun, type Runs } from "./runs";

const chunk = (conversationId: string, p: Partial<StreamChunkPayload>): StreamChunkPayload => ({
  conversationId,
  messageId: "m",
  done: false,
  ...p,
});

const tool = (id: string, status: string): ToolExecutionRecord =>
  ({ id, toolName: "write_file", status, argumentsJson: "{}", output: "" }) as unknown as ToolExecutionRecord;

describe("대화별 실행", () => {
  it("두 대화의 스트림이 서로 섞이지 않는다", () => {
    let runs: Runs = {};
    runs = startRun(runs, "a", "chat", { question: "질문 A" });
    runs = startRun(runs, "b", "project");
    runs = applyChunk(runs, chunk("a", { deltaContent: "안녕" }));
    runs = applyChunk(runs, chunk("b", { deltaReasoning: "생각", deltaContent: "보고서" }));
    runs = applyChunk(runs, chunk("a", { deltaContent: "하세요", statusNotice: "재시도 중" }));
    expect(runs.a.content).toBe("안녕하세요");
    expect(runs.a.notice).toBe("재시도 중");
    expect(runs.a.question).toBe("질문 A");
    expect(runs.b).toMatchObject({ content: "보고서", reasoning: "생각", notice: "" });
  });

  it("끝난 대화에 늦게 온 조각은 버린다", () => {
    let runs: Runs = startRun({}, "a", "chat");
    runs = endRun(runs, "a");
    expect(applyChunk(runs, chunk("a", { deltaContent: "늦음" }))).toEqual({});
    expect(endRun(runs, "a")).toBe(runs);
  });

  it("같은 도구 기록은 상태만 바꾼다", () => {
    let runs: Runs = startRun({}, "a", "code");
    runs = applyChunk(runs, chunk("a", { toolRecord: tool("t1", "running") }));
    runs = applyChunk(runs, chunk("a", { toolRecord: tool("t2", "running") }));
    runs = applyChunk(runs, chunk("a", { toolRecord: tool("t1", "completed") }));
    expect(runs.a.tools.map((t) => `${t.id}:${t.status}`)).toEqual(["t1:completed", "t2:running"]);
  });

  it("화면별·프로젝트별로 진행 중인 수를 센다", () => {
    let runs: Runs = {};
    runs = startRun(runs, "a", "chat");
    runs = startRun(runs, "b", "project");
    runs = startRun(runs, "c", "project");
    runs = startRun(runs, "d", "code");
    expect(countByMode(runs)).toEqual({ chat: 1, project: 2, code: 1 });
    expect(countRunning(runs, ["b", "x", "d"])).toBe(2);
  });
});
