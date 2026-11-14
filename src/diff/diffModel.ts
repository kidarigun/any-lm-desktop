import { diffLines, diffWordsWithSpace } from "diff";

/** 한 줄 안에서 바뀐 부분을 강조하기 위한 조각 */
export interface Segment {
  text: string;
  changed: boolean;
}

export interface DiffRow {
  kind: "ctx" | "add" | "del";
  oldNo: number | null;
  newNo: number | null;
  segments: Segment[];
}

/** 나란히 보기의 한 줄: 왼쪽(이전)·오른쪽(이후) */
export interface SplitRow {
  left: DiffRow | null;
  right: DiffRow | null;
}

export interface DiffResult {
  rows: DiffRow[];
  added: number;
  removed: number;
}

/** 이 길이를 넘는 줄은 단어 단위 강조를 하지 않는다 (느려지지 않도록). */
const WORD_DIFF_MAX_LINE = 2000;
/** 줄의 대부분이 바뀌었으면 단어 강조가 오히려 읽기 어렵다. */
const WORD_DIFF_MAX_CHANGED_RATIO = 0.6;

function splitLines(value: string): string[] {
  const lines = value.split("\n");
  if (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
  return lines;
}

function whole(text: string): Segment[] {
  return [{ text, changed: false }];
}

/** 짝지은 삭제·추가 줄에서 바뀐 단어만 강조한다. */
function wordSegments(oldLine: string, newLine: string): { del: Segment[]; add: Segment[] } | null {
  if (oldLine.length > WORD_DIFF_MAX_LINE || newLine.length > WORD_DIFF_MAX_LINE) return null;
  const parts = diffWordsWithSpace(oldLine, newLine);
  const del: Segment[] = [];
  const add: Segment[] = [];
  let changed = 0;
  for (const p of parts) {
    if (p.added) {
      add.push({ text: p.value, changed: true });
      changed += p.value.length;
    } else if (p.removed) {
      del.push({ text: p.value, changed: true });
      changed += p.value.length;
    } else {
      del.push({ text: p.value, changed: false });
      add.push({ text: p.value, changed: false });
    }
  }
  const total = Math.max(1, oldLine.length + newLine.length);
  if (changed / total > WORD_DIFF_MAX_CHANGED_RATIO) return null;
  return { del, add };
}

/** 두 텍스트를 줄 단위로 비교한다. 줄바꿈(CRLF/LF) 차이는 무시한다. */
export function computeDiff(before: string, after: string): DiffResult {
  const a = before.replace(/\r\n/g, "\n");
  const b = after.replace(/\r\n/g, "\n");
  const rows: DiffRow[] = [];
  let oldNo = 1;
  let newNo = 1;
  let added = 0;
  let removed = 0;

  const parts = diffLines(a, b);
  for (let i = 0; i < parts.length; i++) {
    const part = parts[i];
    const lines = splitLines(part.value);
    if (part.removed) {
      // 바로 뒤에 추가된 줄이 있으면 짝지어 단어 단위로 강조한다.
      const next = parts[i + 1];
      const addLines = next?.added ? splitLines(next.value) : [];
      const delRows: DiffRow[] = [];
      const addRows: DiffRow[] = [];
      lines.forEach((text, k) => {
        const pair = k < addLines.length ? wordSegments(text, addLines[k]) : null;
        delRows.push({ kind: "del", oldNo: oldNo++, newNo: null, segments: pair ? pair.del : whole(text) });
        if (pair) addRows[k] = { kind: "add", oldNo: null, newNo: 0, segments: pair.add };
      });
      addLines.forEach((text, k) => {
        addRows[k] = { kind: "add", oldNo: null, newNo: newNo++, segments: addRows[k]?.segments ?? whole(text) };
      });
      rows.push(...delRows, ...addRows);
      removed += delRows.length;
      added += addRows.length;
      if (next?.added) i++;
    } else if (part.added) {
      for (const text of lines) rows.push({ kind: "add", oldNo: null, newNo: newNo++, segments: whole(text) });
      added += lines.length;
    } else {
      for (const text of lines) rows.push({ kind: "ctx", oldNo: oldNo++, newNo: newNo++, segments: whole(text) });
    }
  }
  return { rows, added, removed };
}

/** 나란히 보기: 삭제 묶음과 추가 묶음을 한 줄씩 짝짓는다. */
export function toSplitRows(rows: DiffRow[]): SplitRow[] {
  const out: SplitRow[] = [];
  let i = 0;
  while (i < rows.length) {
    const r = rows[i];
    if (r.kind === "ctx") {
      out.push({ left: r, right: r });
      i++;
      continue;
    }
    const dels: DiffRow[] = [];
    const adds: DiffRow[] = [];
    while (i < rows.length && rows[i].kind === "del") dels.push(rows[i++]);
    while (i < rows.length && rows[i].kind === "add") adds.push(rows[i++]);
    for (let k = 0; k < Math.max(dels.length, adds.length); k++) {
      out.push({ left: dels[k] ?? null, right: adds[k] ?? null });
    }
  }
  return out;
}

export type Collapsed<T> =
  | { type: "item"; item: T }
  | { type: "gap"; key: string; items: T[] };

/**
 * 바뀐 곳 주변 `context`줄만 남기고 나머지 변경 없는 줄은 접는다.
 * `minGap`줄보다 짧은 구간은 접지 않는다 (펼치기 버튼이 더 거추장스러우므로).
 */
export function collapseContext<T>(
  items: T[],
  isContext: (t: T) => boolean,
  context = 3,
  minGap = 4
): Collapsed<T>[] {
  const changed = items.map((t) => !isContext(t));
  const keep = new Array<boolean>(items.length).fill(false);
  changed.forEach((c, i) => {
    if (!c) return;
    for (let k = Math.max(0, i - context); k <= Math.min(items.length - 1, i + context); k++) keep[k] = true;
  });
  const out: Collapsed<T>[] = [];
  let i = 0;
  while (i < items.length) {
    if (keep[i]) {
      out.push({ type: "item", item: items[i] });
      i++;
      continue;
    }
    let j = i;
    while (j < items.length && !keep[j]) j++;
    const run = items.slice(i, j);
    if (run.length < minGap) run.forEach((item) => out.push({ type: "item", item }));
    else out.push({ type: "gap", key: `gap-${i}`, items: run });
    i = j;
  }
  return out;
}
