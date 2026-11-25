import type { LmModel, ModelPref } from "../types";

/** "9B · Q4_K_M · 6.1GB" 형태의 한 줄 설명 */
export function describeModel(m: LmModel): string {
  const parts: string[] = [];
  if (m.params) parts.push(m.params);
  if (m.quantization) parts.push(m.quantization);
  if (m.sizeBytes > 0) parts.push(`${(m.sizeBytes / 1024 ** 3).toFixed(1)}GB`);
  if (parts.length === 0 && m.key !== m.displayName) return m.key;
  return parts.join(" · ");
}

/** 로드 상태 문구. 로드된 인스턴스가 여러 개면 첫 번째 기준. */
export function loadedLabel(m: LmModel, supportsLoad = true): string {
  const inst = m.loaded[0];
  if (!supportsLoad) {
    if (!inst?.contextLength) return "사용 가능";
    return `사용 가능 · ${Math.round(inst.contextLength / 1024)}K`;
  }
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

export const CONTEXT_PRESETS = [
  8192,
  16384,
  24576,
  32768,
  49152,
  65536,
  98304,
  131072,
] as const;

/** 토큰 수를 "16K"처럼 읽기 쉽게 표시(1024 배수가 아니면 쉼표 숫자). */
export function formatContextSize(tokens: number): string {
  if (tokens > 0 && tokens % 1024 === 0) return `${tokens / 1024}K`;
  return tokens.toLocaleString();
}

/**
 * 컨텍스트 콤보 선택지(8K~128K).
 * - maxContextLength > 0이면 그 이하 프리셋만 노출한다(8K 미만 모델은 maxContextLength 자체를 포함).
 * - 기존 저장값이나 [4K로 다시 시도]로 설정된 currentValue가 프리셋에 없더라도 유효(>= MIN_CONTEXT)하면 오름차순으로 함께 보여 준다.
 */
export function contextOptions(maxContextLength: number, currentValue?: number): number[] {
  const list: number[] = CONTEXT_PRESETS.filter(
    (v) => maxContextLength <= 0 || v <= maxContextLength,
  );
  if (list.length === 0 && maxContextLength >= MIN_CONTEXT) {
    list.push(maxContextLength);
  }
  if (currentValue !== undefined && Number.isFinite(currentValue)) {
    const normalized =
      maxContextLength > 0 ? Math.min(Math.floor(currentValue), maxContextLength) : Math.floor(currentValue);
    if (normalized >= MIN_CONTEXT && !list.includes(normalized)) {
      list.push(normalized);
    }
  }
  return list.sort((a, b) => a - b);
}

/** 입력한 컨텍스트 글자를 숫자로. 비었거나 숫자가 아니거나 1024 미만이면 null(미설정), 최대를 넘으면 최대로. */
export function normalizeContext(raw: string, max: number): number | null {
  const n = Math.floor(Number(raw.trim()));
  if (raw.trim() === "" || !Number.isFinite(n) || n < MIN_CONTEXT) return null;
  return max > 0 ? Math.min(n, max) : n;
}

/** "생각하기" 토글 상태. 저장된 값이 없으면 모델 기본(켜짐)이다. */
export function reasoningEnabled(pref: ModelPref | undefined): boolean {
  return pref?.reasoning !== "off";
}

/** 모델별 설정에 추론 켜기/끄기를 저장한 새 값을 만든다(다른 설정은 그대로). */
export function withReasoning(
  prefs: Record<string, ModelPref>,
  key: string,
  value: "on" | "off",
): Record<string, ModelPref> {
  return { ...prefs, [key]: { ...prefs[key], reasoning: value } };
}
