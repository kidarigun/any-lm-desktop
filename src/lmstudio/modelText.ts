import type { LmModel } from "../types";

/** "9B · Q4_K_M · 6.1GB" 형태의 한 줄 설명 */
export function describeModel(m: LmModel): string {
  const parts: string[] = [];
  if (m.params) parts.push(m.params);
  if (m.quantization) parts.push(m.quantization);
  parts.push(`${(m.sizeBytes / 1024 ** 3).toFixed(1)}GB`);
  return parts.join(" · ");
}

/** 로드 상태 문구. 로드된 인스턴스가 여러 개면 첫 번째 기준. */
export function loadedLabel(m: LmModel): string {
  const inst = m.loaded[0];
  if (!inst) return "로드 안 됨";
  if (!inst.contextLength) return "로드됨";
  return `로드됨 · ${Math.round(inst.contextLength / 1024)}K`;
}

export function capabilityBadges(m: LmModel): ("vision" | "tools" | "reasoning")[] {
  const out: ("vision" | "tools" | "reasoning")[] = [];
  if (m.vision) out.push("vision");
  if (m.toolUse) out.push("tools");
  if (m.reasoningOptions.length > 0) out.push("reasoning");
  return out;
}

export const MIN_CONTEXT = 1024;

/** 입력한 컨텍스트 글자를 숫자로. 비었거나 숫자가 아니거나 1024 미만이면 null(미설정), 최대를 넘으면 최대로. */
export function normalizeContext(raw: string, max: number): number | null {
  const n = Math.floor(Number(raw.trim()));
  if (raw.trim() === "" || !Number.isFinite(n) || n < MIN_CONTEXT) return null;
  return max > 0 ? Math.min(n, max) : n;
}
