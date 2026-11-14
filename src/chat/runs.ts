import type { StreamChunkPayload, ToolExecutionRecord } from "../types";

/** 대화가 시작된 화면 */
export type RunMode = "chat" | "project" | "code";

/** 한 대화에서 진행 중인 답변 생성 */
export interface RunState {
  conversationId: string;
  mode: RunMode;
  reasoning: string;
  content: string;
  tools: ToolExecutionRecord[];
  notice: string;
  /** 화면을 새로 고친 뒤 되살린 실행: 결과는 저장소에서 다시 읽는다 */
  restored?: boolean;
  /** 보낸 질문 (답변이 끝나 저장되기 전까지 화면에 보여 준다) */
  question?: string;
}

/** 대화 ID → 진행 중인 실행 */
export type Runs = Readonly<Record<string, RunState>>;

export function startRun(
  runs: Runs,
  conversationId: string,
  mode: RunMode,
  opts: { notice?: string; restored?: boolean; question?: string } = {}
): Runs {
  return {
    ...runs,
    [conversationId]: {
      conversationId,
      mode,
      reasoning: "",
      content: "",
      tools: [],
      notice: opts.notice ?? "",
      restored: opts.restored,
      question: opts.question,
    },
  };
}

export function endRun(runs: Runs, conversationId: string): Runs {
  if (!(conversationId in runs)) return runs;
  const next = { ...runs };
  delete next[conversationId];
  return next;
}

/**
 * 스트림 조각을 해당 대화의 실행에 반영한다.
 * 이미 끝난(또는 모르는) 대화의 늦게 도착한 조각은 버린다.
 */
export function applyChunk(runs: Runs, p: StreamChunkPayload): Runs {
  const run = runs[p.conversationId];
  if (!run) return runs;
  let tools = run.tools;
  if (p.toolRecord) {
    const i = tools.findIndex((t) => t.id === p.toolRecord!.id);
    tools = i >= 0 ? tools.map((t, j) => (j === i ? p.toolRecord! : t)) : [...tools, p.toolRecord];
  }
  return {
    ...runs,
    [p.conversationId]: {
      ...run,
      reasoning: run.reasoning + (p.deltaReasoning ?? ""),
      content: run.content + (p.deltaContent ?? ""),
      tools,
      notice: p.statusNotice ?? run.notice,
    },
  };
}

/** 화면별 진행 중인 대화 수 (탭 표시용) */
export function countByMode(runs: Runs): Record<RunMode, number> {
  const counts: Record<RunMode, number> = { chat: 0, project: 0, code: 0 };
  for (const r of Object.values(runs)) counts[r.mode] += 1;
  return counts;
}

/** 대화 ID 목록 중 진행 중인 것의 수 (프로젝트 표시용) */
export function countRunning(runs: Runs, conversationIds: Iterable<string>): number {
  let n = 0;
  for (const id of conversationIds) if (id in runs) n += 1;
  return n;
}
