import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import JSZip from "jszip";
import { describe, expect, it } from "vitest";
import { bytesToBase64 } from "./base64";
import { handleDocBridgeRequest } from "./convert";
import {
  compactMarkdownTables,
  headingLevelOfStyle,
  htmlToHwpx,
  hwpxToHtml,
  hwpxToMarkdown,
  looksLikeHwpx,
  markdownToHwpx,
  readPageSetup,
  replaceTextInHwpx,
} from "./hwpx";

// 1×1 PNG
const PNG_1PX =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

const SAMPLE_MD = `# 2026년 사업 계획

## 1. 개요

본 계획은 **핵심 목표**와 *추진 일정*을 정리한 것입니다. <u>밑줄</u>과 ~~취소선~~, \`코드\`도 씁니다.

- 첫째 항목
- 둘째 항목
  - 하위 항목
1. 준비
2. 실행

> 인용한 문장입니다.

| 구분 | 예산(억 원) | 비고 |
| --- | --- | --- |
| 인건비 | 120 | 정규직 |
| 장비 | 35.5 | |

\`\`\`
print("안녕")
\`\`\`

---

마지막 문단.
`;

/** EXA_HWPX_OUT 폴더를 주면 만든 파일을 남긴다 (한글·이음오피스로 열어 확인할 때). */
function keep(name: string, bytes: Uint8Array) {
  const dir = process.env.EXA_HWPX_OUT;
  if (!dir) return;
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, name), bytes);
}

async function entries(bytes: Uint8Array) {
  const zip = await JSZip.loadAsync(bytes);
  return { zip, names: Object.keys(zip.files) };
}

describe("hwpx", () => {
  it("writes a valid package: stored mimetype first, manifest and section", async () => {
    const bytes = await markdownToHwpx(SAMPLE_MD, "사업 계획");
    keep("sample.hwpx", bytes);
    expect(looksLikeHwpx(bytes)).toBe(true);
    const { zip, names } = await entries(bytes);
    expect(names[0]).toBe("mimetype");
    expect(await zip.file("mimetype")!.async("string")).toBe("application/hwp+zip");
    // mimetype은 압축하지 않는다 (로컬 헤더의 압축 방식 0)
    expect(bytes[8]).toBe(0);
    for (const n of ["Contents/content.hpf", "Contents/header.xml", "Contents/section0.xml", "META-INF/container.xml", "version.xml"]) {
      expect(names).toContain(n);
    }
    const section = await zip.file("Contents/section0.xml")!.async("string");
    const parsed = new DOMParser().parseFromString(section, "application/xml");
    expect(parsed.getElementsByTagName("parsererror").length).toBe(0);
    // 구역 설정은 첫 문단에
    expect(section.indexOf("<hp:secPr")).toBeLessThan(section.indexOf("</hp:p>"));
    expect(section).toContain("<hp:tbl");
    const header = await zip.file("Contents/header.xml")!.async("string");
    const headerDoc = new DOMParser().parseFromString(header, "application/xml");
    expect(headerDoc.getElementsByTagName("parsererror").length).toBe(0);
    // 개수 속성이 실제 항목 수와 같아야 한다
    for (const [list, item] of [
      ["charProperties", "charPr"],
      ["paraProperties", "paraPr"],
      ["styles", "style"],
      ["borderFills", "borderFill"],
      ["numberings", "numbering"],
    ]) {
      const el = headerDoc.getElementsByTagName(`hh:${list}`)[0];
      expect(Number(el.getAttribute("itemCnt"))).toBe(el.getElementsByTagName(`hh:${item}`).length);
    }
    // 문단이 참조하는 모양 번호가 모두 header에 있다
    const charCount = headerDoc.getElementsByTagName("hh:charPr").length;
    const paraCount = headerDoc.getElementsByTagName("hh:paraPr").length;
    for (const m of section.matchAll(/charPrIDRef="(\d+)"/g)) expect(Number(m[1])).toBeLessThan(charCount);
    for (const m of section.matchAll(/paraPrIDRef="(\d+)"/g)) expect(Number(m[1])).toBeLessThan(paraCount);
  });

  it("reads back headings, formatting, lists, tables and code", async () => {
    const bytes = await markdownToHwpx(SAMPLE_MD, "사업 계획");
    const md = await hwpxToMarkdown(bytes);
    expect(md).toContain("# 2026년 사업 계획");
    expect(md).toMatch(/## 1\\?\. 개요/);
    expect(md).toContain("**핵심 목표**");
    expect(md).toContain("*추진 일정*");
    expect(md).toContain("<u>밑줄</u>");
    expect(md).toMatch(/-\s+첫째 항목/);
    expect(md).toMatch(/\s+-\s+하위 항목/);
    expect(md).toMatch(/1\.\s+준비/);
    expect(md).toMatch(/2\.\s+실행/);
    expect(md).toContain("| 구분 | 예산(억 원) | 비고 |");
    expect(md).toMatch(/~~?취소선~~?/);
    expect(md).toContain("| 인건비 | 120 | 정규직 |");
    expect(md).toContain('print("안녕")');
    expect(md).toContain("마지막 문단.");
  });

  it("restarts numbering for separate ordered lists", async () => {
    const bytes = await markdownToHwpx("1. 가\n2. 나\n\n문단\n\n1. 다\n2. 라\n");
    const { zip } = await entries(bytes);
    const header = await zip.file("Contents/header.xml")!.async("string");
    expect(header).toContain('<hh:numberings itemCnt="2">');
    const html = await hwpxToHtml(bytes);
    expect(html.match(/<ol>/g)?.length).toBe(2);
  });

  it("embeds images from data URLs and reads them back", async () => {
    const bytes = await htmlToHwpx(`<p>그림:</p><img src="data:image/png;base64,${PNG_1PX}">`, "그림");
    keep("image.hwpx", bytes);
    const { zip, names } = await entries(bytes);
    expect(names).toContain("BinData/image1.png");
    const hpf = await zip.file("Contents/content.hpf")!.async("string");
    expect(hpf).toContain('href="BinData/image1.png"');
    const html = await hwpxToHtml(bytes);
    expect(html).toMatch(/<img src="data:image\/png;base64,/);
    // 모델에게는 그림 데이터 대신 파일 이름만
    expect(await hwpxToMarkdown(bytes)).toContain("image1.png");
  });

  it("treats the editor's line break in an empty cell as empty", async () => {
    const html =
      '<table><tbody><tr><th><p>구분</p></th><th><p>비고</p></th></tr>' +
      '<tr><td><p>장비</p></td><td><p><br class="ProseMirror-trailingBreak"></p></td></tr></tbody></table>';
    const md = await hwpxToMarkdown(await htmlToHwpx(html));
    expect(md).toMatch(/\| 장비 \|\s*\|\n/);
    expect(md).not.toContain("<br");
  });

  it("keeps merged cells", async () => {
    const html =
      '<table><tr><th colspan="2">합계</th></tr><tr><td rowspan="2">가</td><td>1</td></tr><tr><td>2</td></tr></table>';
    const bytes = await htmlToHwpx(html);
    const { zip } = await entries(bytes);
    const section = await zip.file("Contents/section0.xml")!.async("string");
    expect(section).toContain('rowCnt="3" colCnt="2"');
    expect(section).toContain('<hp:cellAddr colAddr="1" rowAddr="2"/>');
    const back = await hwpxToHtml(bytes);
    expect(back).toContain('colspan="2"');
    expect(back).toContain('rowspan="2"');
  });

  it("replaces text inside one paragraph and keeps its formatting", async () => {
    const bytes = await markdownToHwpx("# 제목\n\n위약금은 **10%** 입니다.\n\n| 항목 | 값 |\n| --- | --- |\n| 기간 | 3년 |\n");
    // 굵은 글씨 안의 문구는 굵은 채로
    const bold = await replaceTextInHwpx(bytes, "10%", "15%");
    expect(await hwpxToMarkdown(bold)).toContain("위약금은 **15%** 입니다.");
    // 글자 묶음 두 개에 걸친 문구는 첫 묶음의 모양을 따른다
    const out = await replaceTextInHwpx(bytes, "위약금은 10%", "위약금은 15%");
    const md = await hwpxToMarkdown(out);
    expect(md).toContain("위약금은 15% 입니다.");
    // 표 칸 안의 문구
    const out2 = await replaceTextInHwpx(out, "3년", "5년");
    expect(await hwpxToMarkdown(out2)).toContain("| 기간 | 5년 |");
    const { names } = await entries(out2);
    expect(names[0]).toBe("mimetype");
    await expect(replaceTextInHwpx(out2, "없는 문구", "x")).rejects.toThrow(/찾지 못했습니다/);
  });

  it("keeps the page setup of the original when saving", async () => {
    const original = await markdownToHwpx("본문");
    const zip = await JSZip.loadAsync(original);
    const section = (await zip.file("Contents/section0.xml")!.async("string")).replace(
      /<hp:pagePr landscape="WIDELY" width="59528" height="84186"/,
      '<hp:pagePr landscape="NARROWLY" width="59528" height="84186"'
    );
    zip.file("Contents/section0.xml", section);
    const landscape = await zip.generateAsync({ type: "uint8array" });
    expect((await readPageSetup(landscape))?.landscape).toBe("NARROWLY");
    const saved = await htmlToHwpx("<p>고친 본문</p>", "문서", { base: landscape });
    expect((await readPageSetup(saved))?.landscape).toBe("NARROWLY");
  });

  it("refuses the binary HWP format with a clear message", async () => {
    const cfb = new Uint8Array([0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1, 0, 0, 0, 0]);
    await expect(hwpxToHtml(cfb)).rejects.toThrow(/HWP\(한글 97~2018/);
    await expect(hwpxToHtml(new Uint8Array([1, 2, 3]))).rejects.toThrow(/HWPX 문서가 아닙니다/);
  });

  it("drops the empty columns and rows of layout tables", () => {
    const md = [
      "앞 문단",
      "",
      "|  |  |  |  |",
      "| --- | --- | --- |",
      "| **1** |  | **서류접수** |  |",
      "|  |  |  |  |",
      "| 2 |  | 서류심사 | 비고\\|메모 |",
      "",
      "|  |  |",
      "| --- | --- |",
      "|  |  |",
      "뒤 문단",
    ].join("\n");
    expect(compactMarkdownTables(md)).toBe(
      ["앞 문단", "", "| **1** | **서류접수** |  |", "| --- | --- | --- |", "| 2 | 서류심사 | 비고\\|메모 |", "", "뒤 문단"].join("\n")
    );
  });

  it("recognizes heading styles by name", () => {
    expect(headingLevelOfStyle({ name: "개요 2", engName: "Outline 2" })).toBe(2);
    expect(headingLevelOfStyle({ name: "제목 1", engName: "" })).toBe(1);
    expect(headingLevelOfStyle({ name: "사용자", engName: "Heading 3" })).toBe(3);
    expect(headingLevelOfStyle({ name: "바탕글", engName: "Normal" })).toBeNull();
  });

  it("is reachable through the document bridge", async () => {
    const gen = await handleDocBridgeRequest({ requestId: "1", op: "generate", kind: "hwpx", markdown: "# 보고서\n\n본문", title: "보고서" });
    expect(gen.ok).toBe(true);
    const ext = await handleDocBridgeRequest({ requestId: "2", op: "extract", kind: "hwpx", dataBase64: gen.dataBase64 });
    expect(ext.text).toContain("# 보고서");
    // Markdown 기호가 섞인 target도 한 번 더 시도한다
    const rep = await handleDocBridgeRequest({
      requestId: "3",
      op: "replace",
      kind: "hwpx",
      dataBase64: gen.dataBase64,
      target: "**본문**",
      replacement: "고친 본문",
    });
    expect(rep.ok).toBe(true);
    const ext2 = await handleDocBridgeRequest({ requestId: "4", op: "extract", kind: "hwpx", dataBase64: rep.dataBase64 });
    expect(ext2.text).toContain("고친 본문");
    expect(bytesToBase64(new Uint8Array([1]))).toBe("AQ==");
  });
});
