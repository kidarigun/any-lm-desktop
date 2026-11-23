import type { MemoryItem } from "../types";

/** 백엔드(memory.rs)와 같은 한도 */
export const MEMORY_ITEM_MAX_CHARS = 500;
export const MEMORY_MAX_ITEMS = 200;

/** 화면에는 새 항목이 위로 온다. 원래 배열은 바꾸지 않는다. */
export function newestFirst(items: MemoryItem[]): MemoryItem[] {
  return [...items].reverse().sort((a, b) => (a.createdAt < b.createdAt ? 1 : a.createdAt > b.createdAt ? -1 : 0));
}

export function memoryStats(items: MemoryItem[]): { count: number; chars: number } {
  return { count: items.length, chars: items.reduce((n, m) => n + [...m.text].length, 0) };
}

export function memorySourceLabel(source: MemoryItem["source"]): string {
  return source === "model" ? "모델" : "직접";
}

/** 저장 전에 화면에서 먼저 거르는 검사. 문제가 없으면 null. */
export function validateMemoryText(text: string): string | null {
  const t = text.trim();
  if (!t) return "기억할 내용이 비어 있습니다.";
  const n = [...t].length;
  if (n > MEMORY_ITEM_MAX_CHARS) return `한 항목은 ${MEMORY_ITEM_MAX_CHARS}자까지입니다 (지금 ${n}자).`;
  return null;
}
