import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import type { HwpDocument } from "@rhwp/core";
import { beforeAll, describe, expect, it } from "vitest";
import { hwpxToMarkdown, markdownToHwpx } from "../../docs/hwpx";
import {
  caretRect,
  deleteBackward,
  deleteForward,
  insertText,
  lineEdge,
  moveHorizontal,
  parseResult,
  posFromRaw,
  splitParagraph,
  type DocPos,
  type RhwpDoc,
} from "./editing";
import { exportHwpx } from "./rhwpEngine";

// 실제 rhwp WASM 엔진으로 시험한다 (Node에서도 돈다).
let rhwp: typeof import("@rhwp/core");

beforeAll(async () => {
  const require = createRequire(import.meta.url);
  rhwp = await import("@rhwp/core");
  await rhwp.default({ module_or_path: readFileSync(require.resolve("@rhwp/core/rhwp_bg.wasm")) });
});

const MD = "# 제목\n\n본문 문단입니다.\n\n| 가 | 나 |\n| --- | --- |\n| 1 | 2 |\n";

async function open(md = MD): Promise<{ doc: HwpDocument; api: RhwpDoc }> {
  const doc = new rhwp.HwpDocument(await markdownToHwpx(md));
  return { doc, api: doc as unknown as RhwpDoc };
}

async function markdownOf(doc: HwpDocument): Promise<string> {
  const { bytes, lossCount } = exportHwpx(doc);
  expect(lossCount).toBe(0);
  return hwpxToMarkdown(bytes);
}

const BODY: DocPos = { section: 0, para: 1, offset: 0 };

describe("한글 문서 편집 (rhwp)", () => {
  it("inserts text, keeps the caret after it and saves as HWPX", async () => {
    const { doc, api } = await open();
    const pos = insertText(api, BODY, "새 ");
    expect(pos.offset).toBe(2);
    expect(await markdownOf(doc)).toContain("새 본문 문단입니다.");
    expect(caretRect(api, pos)?.page).toBe(0);
  });

  it("splits with Enter and merges back with Backspace", async () => {
    const { doc, api } = await open();
    const before = api.getParagraphCount(0);
    const split = splitParagraph(api, { ...BODY, offset: 2 });
    expect(split).toEqual({ section: 0, para: 2, offset: 0 });
    expect(api.getParagraphCount(0)).toBe(before + 1);
    expect(await markdownOf(doc)).toMatch(/본문\n\n문단입니다\./);

    const merged = deleteBackward(api, split);
    expect(merged).toEqual({ section: 0, para: 1, offset: 2 });
    expect(api.getParagraphCount(0)).toBe(before);
    expect(await markdownOf(doc)).toContain("본문 문단입니다.");
  });

  it("deletes one character either way and stops at the document start", async () => {
    const { doc, api } = await open();
    expect(deleteForward(api, BODY)).toEqual(BODY);
    const back = deleteBackward(api, { ...BODY, offset: 2 });
    expect(back?.offset).toBe(1);
    // Delete로 "본", Backspace로 그다음 빈칸을 지웠다.
    expect(await markdownOf(doc)).toContain("문문단입니다.");
    expect(deleteBackward(api, { section: 0, para: 0, offset: 0 })).toBeNull();
  });

  it("multi-line text becomes separate paragraphs", async () => {
    const { doc, api } = await open();
    const end = insertText(api, BODY, "첫 줄\n둘째 줄\n");
    expect(end.para).toBe(3);
    const md = await markdownOf(doc);
    expect(md).toMatch(/첫 줄\n\n둘째 줄\n\n본문 문단입니다\./);
  });

  it("moves across paragraph boundaries with the arrow keys", async () => {
    const { api } = await open();
    const len = api.getParagraphLength(0, 1);
    expect(moveHorizontal(api, { ...BODY, offset: len }, 1)).toEqual({ section: 0, para: 2, offset: 0 });
    expect(moveHorizontal(api, BODY, -1)).toEqual({ section: 0, para: 0, offset: api.getParagraphLength(0, 0) });
    expect(lineEdge(api, { ...BODY, offset: 3 }, "start").offset).toBe(0);
    expect(lineEdge(api, { ...BODY, offset: 3 }, "end").offset).toBe(len);
  });

  it("edits text inside a table cell", async () => {
    const { doc, api } = await open();
    // 표는 세 번째 문단(2)의 첫 컨트롤, 칸은 행 우선 순서 (가, 나, 1, 2)
    const cell: DocPos = { section: 0, para: 2, offset: 0, path: [{ controlIndex: 0, cellIndex: 2, cellParaIndex: 0 }] };
    expect(caretRect(api, cell)).not.toBeNull();
    const after = insertText(api, cell, "제");
    expect(after.offset).toBe(1);
    expect(await markdownOf(doc)).toContain("| 제1 | 2 |");
    const split = splitParagraph(api, after);
    expect(split.path?.[0].cellParaIndex).toBe(1);
    expect(deleteBackward(api, split)?.path?.[0].cellParaIndex).toBe(0);
    // 칸의 처음에서는 칸 밖으로 나가지 않는다
    expect(deleteBackward(api, cell)).toBeNull();
    expect(moveHorizontal(api, cell, -1)).toEqual(cell);
  });

  it("reads positions from hit tests in the body and in cells", () => {
    expect(posFromRaw({ sectionIndex: 0, paragraphIndex: 3, charOffset: 4 })).toEqual({ section: 0, para: 3, offset: 4 });
    expect(
      posFromRaw({
        sectionIndex: 0,
        paragraphIndex: 0,
        charOffset: 1,
        parentParaIndex: 42,
        cellPath: [{ controlIndex: 0, cellIndex: 35, cellParaIndex: 0 }],
      })
    ).toEqual({ section: 0, para: 42, offset: 1, path: [{ controlIndex: 0, cellIndex: 35, cellParaIndex: 0 }] });
    expect(posFromRaw({})).toBeNull();
    expect(() => parseResult('{"ok":false,"error":"범위 밖"}')).toThrow("범위 밖");
  });
});
