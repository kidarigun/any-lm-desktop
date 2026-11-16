/**
 * rhwp 문서 위에서의 캐럿 이동과 글자 편집.
 *
 * rhwp(@rhwp/core)는 한글 문서를 원래 모양대로 배치·렌더링하고, 문단·표 칸 단위의 편집 API를 준다.
 * 여기서는 캐럿 위치(본문 문단 또는 표 칸 안 문단)를 하나의 모양(DocPos)으로 다루고,
 * 본문/표 칸에 따라 알맞은 API를 부른다. API는 JSON 문자열을 주고받는다.
 */

/** 표 칸 경로의 한 단계 (중첩 표면 여러 단계) */
export interface CellStep {
  controlIndex: number;
  cellIndex: number;
  cellParaIndex: number;
}

/** 캐럿 위치. path가 있으면 표 칸 안이고, 문단 번호는 path 마지막 단계의 cellParaIndex다. */
export interface DocPos {
  section: number;
  /** 본문 문단 번호 (표 칸 안이면 표가 든 본문 문단 번호) */
  para: number;
  offset: number;
  path?: CellStep[];
}

export interface CaretRect {
  page: number;
  x: number;
  y: number;
  height: number;
}

/** 이 모듈이 쓰는 rhwp HwpDocument의 메서드 */
export interface RhwpDoc {
  getSectionCount(): number;
  getParagraphCount(section: number): number;
  getParagraphLength(section: number, para: number): number;
  getCellParagraphCountByPath(section: number, parentPara: number, pathJson: string): number;
  getCellParagraphLengthByPath(section: number, parentPara: number, pathJson: string): number;
  getCursorRect(section: number, para: number, offset: number): string;
  getCursorRectByPath(section: number, parentPara: number, pathJson: string, offset: number): string;
  insertText(section: number, para: number, offset: number, text: string): string;
  insertTextInCellByPath(section: number, parentPara: number, pathJson: string, offset: number, text: string): string;
  deleteText(section: number, para: number, offset: number, count: number): string;
  deleteTextInCellByPath(section: number, parentPara: number, pathJson: string, offset: number, count: number): string;
  splitParagraph(section: number, para: number, offset: number): string;
  splitParagraphInCellByPath(section: number, parentPara: number, pathJson: string, offset: number): string;
  mergeParagraph(section: number, para: number): string;
  mergeParagraphInCellByPath(section: number, parentPara: number, pathJson: string): string;
  moveVertical(
    section: number,
    para: number,
    offset: number,
    delta: number,
    preferredX: number,
    parentPara: number,
    controlIndex: number,
    cellIndex: number,
    cellPara: number
  ): string;
  moveVerticalByPath(section: number, parentPara: number, pathJson: string, offset: number, delta: number, preferredX: number): string;
  getLineInfo(section: number, para: number, offset: number): string;
  getLineInfoInCell(section: number, parentPara: number, controlIndex: number, cellIndex: number, cellPara: number, offset: number): string;
}

/** 본문 문단에서 moveVertical에 넘기는 "표 칸 아님" 값 */
const NONE = 0xffffffff;

/** rhwp의 JSON 결과를 읽는다. {"ok":false}나 {"error":…}면 오류를 던진다. */
export function parseResult<T = Record<string, unknown>>(json: string): T {
  let v: unknown;
  try {
    v = JSON.parse(json);
  } catch {
    throw new Error(`한글 문서 엔진의 응답을 해석할 수 없습니다: ${json.slice(0, 120)}`);
  }
  const o = v as { ok?: boolean; error?: string };
  if (o && typeof o === "object" && (o.ok === false || typeof o.error === "string")) {
    throw new Error(o.error || "한글 문서 엔진이 요청을 처리하지 못했습니다.");
  }
  return v as T;
}

interface RawPos {
  sectionIndex?: number;
  paragraphIndex?: number;
  charOffset?: number;
  parentParaIndex?: number;
  cellPath?: CellStep[];
  controlIndex?: number;
  cellIndex?: number;
  cellParaIndex?: number;
}

/** hitTest·moveVertical 결과에서 캐럿 위치를 꺼낸다. */
export function posFromRaw(raw: RawPos): DocPos | null {
  if (typeof raw.sectionIndex !== "number") return null;
  const offset = raw.charOffset ?? 0;
  if (typeof raw.parentParaIndex === "number" && raw.parentParaIndex !== NONE) {
    let path = raw.cellPath?.map((s) => ({
      controlIndex: s.controlIndex,
      cellIndex: s.cellIndex,
      cellParaIndex: s.cellParaIndex,
    }));
    if ((!path || path.length === 0) && typeof raw.cellIndex === "number") {
      path = [{ controlIndex: raw.controlIndex ?? 0, cellIndex: raw.cellIndex, cellParaIndex: raw.cellParaIndex ?? 0 }];
    }
    if (path && path.length > 0) return { section: raw.sectionIndex, para: raw.parentParaIndex, offset, path };
  }
  return { section: raw.sectionIndex, para: raw.paragraphIndex ?? 0, offset };
}

export function inCell(p: DocPos): p is DocPos & { path: CellStep[] } {
  return !!p.path && p.path.length > 0;
}

function pathJson(p: DocPos): string {
  return JSON.stringify(p.path ?? []);
}

/** 표 칸 안 문단 번호를 바꾼 경로 */
function withCellPara(p: DocPos, cellPara: number): DocPos {
  const path = (p.path ?? []).slice();
  path[path.length - 1] = { ...path[path.length - 1], cellParaIndex: cellPara };
  return { ...p, path };
}

function cellPara(p: DocPos): number {
  return p.path ? p.path[p.path.length - 1].cellParaIndex : 0;
}

export function paragraphLength(doc: RhwpDoc, p: DocPos): number {
  return inCell(p)
    ? doc.getCellParagraphLengthByPath(p.section, p.para, pathJson(p))
    : doc.getParagraphLength(p.section, p.para);
}

function paragraphCount(doc: RhwpDoc, p: DocPos): number {
  return inCell(p) ? doc.getCellParagraphCountByPath(p.section, p.para, pathJson(p)) : doc.getParagraphCount(p.section);
}

export function caretRect(doc: RhwpDoc, p: DocPos): CaretRect | null {
  try {
    const r = parseResult<{ pageIndex: number; x: number; y: number; height: number }>(
      inCell(p) ? doc.getCursorRectByPath(p.section, p.para, pathJson(p), p.offset) : doc.getCursorRect(p.section, p.para, p.offset)
    );
    return { page: r.pageIndex, x: r.x, y: r.y, height: r.height };
  } catch {
    return null;
  }
}

/** 글자를 넣는다. 줄바꿈이 있으면 문단을 나눠 가며 넣는다. 넣은 뒤의 캐럿 위치를 돌려준다. */
export function insertText(doc: RhwpDoc, p: DocPos, text: string): DocPos {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  let pos = p;
  lines.forEach((line, i) => {
    if (i > 0) pos = splitParagraph(doc, pos);
    if (!line) return;
    const r = parseResult<{ charOffset: number }>(
      inCell(pos)
        ? doc.insertTextInCellByPath(pos.section, pos.para, pathJson(pos), pos.offset, line)
        : doc.insertText(pos.section, pos.para, pos.offset, line)
    );
    pos = { ...pos, offset: r.charOffset };
  });
  return pos;
}

/** Enter: 캐럿 자리에서 문단을 나눈다. */
export function splitParagraph(doc: RhwpDoc, p: DocPos): DocPos {
  if (inCell(p)) {
    const r = parseResult<{ cellParaIndex: number; charOffset: number }>(
      doc.splitParagraphInCellByPath(p.section, p.para, pathJson(p), p.offset)
    );
    return { ...withCellPara(p, r.cellParaIndex), offset: r.charOffset ?? 0 };
  }
  const r = parseResult<{ paraIdx: number; charOffset: number }>(doc.splitParagraph(p.section, p.para, p.offset));
  return { ...p, para: r.paraIdx, offset: r.charOffset ?? 0 };
}

/** Backspace. 문단 처음이면 앞 문단과 합친다. 바뀐 것이 없으면 null. */
export function deleteBackward(doc: RhwpDoc, p: DocPos): DocPos | null {
  if (p.offset > 0) {
    const at = p.offset - 1;
    const r = parseResult<{ charOffset: number }>(
      inCell(p) ? doc.deleteTextInCellByPath(p.section, p.para, pathJson(p), at, 1) : doc.deleteText(p.section, p.para, at, 1)
    );
    return { ...p, offset: r.charOffset ?? at };
  }
  if (inCell(p)) {
    if (cellPara(p) === 0) return null;
    const r = parseResult<{ cellParaIndex: number; charOffset: number }>(
      doc.mergeParagraphInCellByPath(p.section, p.para, pathJson(p))
    );
    return { ...withCellPara(p, r.cellParaIndex), offset: r.charOffset };
  }
  if (p.para === 0) return null;
  const r = parseResult<{ paraIdx: number; charOffset: number }>(doc.mergeParagraph(p.section, p.para));
  return { ...p, para: r.paraIdx, offset: r.charOffset };
}

/** Delete. 문단 끝이면 다음 문단을 끌어와 합친다. 바뀐 것이 없으면 null. */
export function deleteForward(doc: RhwpDoc, p: DocPos): DocPos | null {
  const len = paragraphLength(doc, p);
  if (p.offset < len) {
    parseResult(
      inCell(p)
        ? doc.deleteTextInCellByPath(p.section, p.para, pathJson(p), p.offset, 1)
        : doc.deleteText(p.section, p.para, p.offset, 1)
    );
    return p;
  }
  const count = paragraphCount(doc, p);
  if (inCell(p)) {
    if (cellPara(p) + 1 >= count) return null;
    parseResult(doc.mergeParagraphInCellByPath(p.section, p.para, pathJson(withCellPara(p, cellPara(p) + 1))));
    return p;
  }
  if (p.para + 1 >= count) return null;
  parseResult(doc.mergeParagraph(p.section, p.para + 1));
  return p;
}

/** ←/→: 한 글자씩. 문단 끝에서는 다음(앞) 문단으로 넘어간다. 표 칸 밖으로는 나가지 않는다. */
export function moveHorizontal(doc: RhwpDoc, p: DocPos, delta: -1 | 1): DocPos {
  const len = paragraphLength(doc, p);
  if (delta < 0 && p.offset > 0) return { ...p, offset: Math.min(p.offset - 1, len) };
  if (delta > 0 && p.offset < len) return { ...p, offset: p.offset + 1 };
  if (inCell(p)) {
    const idx = cellPara(p) + delta;
    if (idx < 0 || idx >= paragraphCount(doc, p)) return p;
    const next = withCellPara(p, idx);
    return { ...next, offset: delta < 0 ? paragraphLength(doc, next) : 0 };
  }
  const para = p.para + delta;
  if (para < 0 || para >= paragraphCount(doc, p)) return p;
  const next = { ...p, para };
  return { ...next, offset: delta < 0 ? paragraphLength(doc, next) : 0 };
}

/** ↑/↓: rhwp가 줄·문단·표 경계를 처리한다. */
export function moveVertical(doc: RhwpDoc, p: DocPos, delta: -1 | 1, preferredX: number): { pos: DocPos; preferredX: number } {
  const raw = parseResult<RawPos & { preferredX?: number }>(
    inCell(p)
      ? doc.moveVerticalByPath(p.section, p.para, pathJson(p), p.offset, delta, preferredX)
      : doc.moveVertical(p.section, p.para, p.offset, delta, preferredX, NONE, NONE, NONE, NONE)
  );
  return { pos: posFromRaw(raw) ?? p, preferredX: raw.preferredX ?? preferredX };
}

/** Home/End: 캐럿이 있는 줄의 처음·끝 */
export function lineEdge(doc: RhwpDoc, p: DocPos, edge: "start" | "end"): DocPos {
  try {
    let json: string;
    if (inCell(p)) {
      if (p.path.length > 1) {
        return { ...p, offset: edge === "start" ? 0 : paragraphLength(doc, p) };
      }
      const s = p.path[0];
      json = doc.getLineInfoInCell(p.section, p.para, s.controlIndex, s.cellIndex, s.cellParaIndex, p.offset);
    } else {
      json = doc.getLineInfo(p.section, p.para, p.offset);
    }
    const info = parseResult<{ charStart: number; charEnd: number; lineIndex: number; lineCount: number }>(json);
    if (edge === "start") return { ...p, offset: info.charStart };
    // 마지막 줄이 아니면 줄 끝 글자(줄바꿈 자리) 앞에 둔다.
    const end = info.lineIndex + 1 < info.lineCount ? Math.max(info.charStart, info.charEnd - 1) : info.charEnd;
    return { ...p, offset: Math.min(end, paragraphLength(doc, p)) };
  } catch {
    return { ...p, offset: edge === "start" ? 0 : paragraphLength(doc, p) };
  }
}
