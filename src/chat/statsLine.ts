import type { TurnStats } from "../types";

/** 초당 출력 토큰 수. 걸린 시간이 0이면 구할 수 없다. */
export function tokensPerSecond(stats: Pick<TurnStats, "completionTokens" | "elapsedMs">): number | null {
  if (!stats.elapsedMs || stats.elapsedMs <= 0) return null;
  return stats.completionTokens / (stats.elapsedMs / 1000);
}

/** 답변 아래 통계 줄: "입력 n · 출력 n · 추론 n토큰 · 초당 n.n토큰 · {모델}" */
export function statsLineText(stats: TurnStats, modelName?: string): string {
  const parts = [
    `입력 ${stats.promptTokens.toLocaleString()}`,
    `출력 ${stats.completionTokens.toLocaleString()}`,
    `추론 ${stats.reasoningTokens.toLocaleString()}토큰`,
  ];
  const tps = tokensPerSecond(stats);
  if (tps !== null) parts.push(`초당 ${tps.toFixed(1)}토큰`);
  if (modelName) parts.push(modelName);
  return parts.join(" · ");
}
