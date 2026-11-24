import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import JSZip from "jszip";
import { describe, expect, it } from "vitest";
import { base64ToBytes, bytesToBase64 } from "./base64";
import { handleDocBridgeRequest } from "./convert";
import { docxToHtml, docxToMarkdown, htmlToDocx, imageSize, markdownToDocx } from "./docx";
import { docKindOf, replaceExtension } from "./formats";
import { htmlToMarkdown, markdownToHtml, stripInlineMarkdown } from "./markdown";
import { configurePdfjs, extractPdfText } from "./pdf";
import {
  applyPptxEdits,
  deckToMarkdown,
  markdownToPptx,
  markdownToSlideSpecs,
  parsePptx,
  replaceTextInOoxml,
  resolvePartPath,
} from "./pptx";

const require = createRequire(import.meta.url);

describe("formats", () => {
  it("detects document kinds and replaces extensions", () => {
    expect(docKindOf("docs/보고서.DOCX")).toBe("docx");
    expect(docKindOf("a/b/deck.pptx")).toBe("pptx");
    expect(docKindOf("note.md")).toBe("markdown");
    expect(docKindOf("main.rs")).toBe("text");
    expect(replaceExtension("보고서.md", "docx")).toBe("보고서.docx");
    expect(resolvePartPath("ppt/slides", "../slideLayouts/slideLayout1.xml")).toBe(
      "ppt/slideLayouts/slideLayout1.xml"
    );
  });

  it("round-trips base64", () => {
    const bytes = new Uint8Array([0, 1, 2, 250, 255]);
    expect(Array.from(base64ToBytes(bytesToBase64(bytes)))).toEqual(Array.from(bytes));
  });
});

describe("markdown", () => {
  it("converts TipTap table HTML to a GFM table", () => {
    const html =
      "<table><colgroup><col></colgroup><tbody><tr><th><p>항목</p></th><th><p>값</p></th></tr>" +
      "<tr><td><p>매출</p></td><td><p><strong>120</strong>억</p></td></tr></tbody></table>";
    const md = htmlToMarkdown(html);
    expect(md).toContain("| 항목 | 값 |");
    expect(md).toContain("| 매출 | **120**억 |");
  });

  it("keeps headings, lists and underline", () => {
    const md = htmlToMarkdown("<h2>제목</h2><ul><li><p>첫째</p></li></ul><p><u>밑줄</u></p>");
    expect(md).toContain("## 제목");
    expect(md).toMatch(/-\s+첫째/);
    expect(md).toContain("<u>밑줄</u>");
  });

  it("writes tight nested lists from editor HTML", () => {
    const html =
      "<ul><li><p>한국어 성능 우수</p><ul><li><p>512K 컨텍스트</p></li></ul></li><li><p>도구 호출 지원</p></li></ul>" +
      "<ol><li><p>첫째</p></li><li><p>둘째</p></li></ol>";
    expect(htmlToMarkdown(html)).toBe("- 한국어 성능 우수\n  - 512K 컨텍스트\n- 도구 호출 지원\n\n1. 첫째\n2. 둘째\n");
  });

  it("strips inline markdown", () => {
    expect(stripInlineMarkdown("## **굵은** `코드`와 [링크](http://x)")).toBe("굵은 코드와 링크");
  });
});

describe("docx", () => {
  it("round-trips markdown through DOCX", async () => {
    const md = [
      "# 2026 사업 계획",
      "",
      "본문 **굵게** 와 *기울임*.",
      "",
      "- 항목 하나",
      "  - 하위 항목",
      "1. 첫 번째",
      "2. 두 번째",
      "",
      "| 구분 | 금액 |",
      "| --- | --- |",
      "| 매출 | 120 |",
    ].join("\n");
    const bytes = await markdownToDocx(md, "계획");
    expect(bytes[0]).toBe(0x50); // PK zip
    const html = await docxToHtml(bytes);
    expect(html).toContain("<h1>2026 사업 계획</h1>");
    expect(html).toContain("<strong>굵게</strong>");
    expect(html).toContain("<em>기울임</em>");
    expect(html).toContain("<table>");
    // 첫 행만 머리글, 나머지는 일반 셀
    expect(html).toContain("<td><p>매출</p></td>");
    expect(html).not.toContain("<th><p>매출</p></th>");
    const back = await docxToMarkdown(bytes);
    expect(back).toContain("# 2026 사업 계획");
    expect(back).toMatch(/-\s+항목 하나/);
    expect(back).toContain("매출");
  });

  it("writes underline, links and code from editor HTML", async () => {
    const bytes = await htmlToDocx(
      '<p><u>밑줄</u> <a href="https://lgresearch.ai">링크</a> <code>x=1</code></p><blockquote><p>인용</p></blockquote><pre><code>let a = 1;\nlet b = 2;</code></pre>'
    );
    const zip = await JSZip.loadAsync(bytes);
    const xml = await zip.file("word/document.xml")!.async("string");
    expect(xml).toContain("<w:u ");
    expect(xml).toContain("w:hyperlink");
    expect(xml).toContain("Consolas");
    expect(xml).toContain("let b = 2;");
    const styles = await zip.file("word/styles.xml")!.async("string");
    expect(styles).toContain("Malgun Gothic");
  });

  it("reads PNG image size", () => {
    const png = base64ToBytes(
      "iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAYAAACddGYaAAAAEUlEQVR42mP8z8Dwn4EIwAgAPwIF/1WrBf8AAAAASUVORK5CYII="
    );
    expect(imageSize(png)).toEqual({ width: 3, height: 2 });
  });

  it("replaces text across runs while keeping formatting", async () => {
    const bytes = await markdownToDocx("올해 **매출 목표**는 100억입니다.");
    const out = await replaceTextInOoxml(bytes, "docx", "매출 목표는 100억", "영업이익 목표는 30억");
    const md = await docxToMarkdown(out);
    // 교체된 구간은 시작 위치 런(굵게)의 서식을 따른다.
    expect(md).toContain("**영업이익 목표는 30억**입니다");
    const xml = await (await JSZip.loadAsync(out)).file("word/document.xml")!.async("string");
    expect(xml).toContain("<w:b"); // 첫 런의 굵게 서식 유지
    await expect(replaceTextInOoxml(bytes, "docx", "없는 문구", "x")).rejects.toThrow(/찾지 못했습니다/);
  });
});

describe("pptx", () => {
  const deckMd = [
    "# 2026 AI 전략",
    "",
    "미래전략기획실",
    "",
    "## 추진 배경",
    "",
    "- 한국어 성능 **우수**",
    "  - 512K 컨텍스트",
    "- 도구 호출 지원",
    "",
    "노트: 배경을 2분 안에 설명",
    "",
    "## 기대 효과",
    "",
    "| 업무 | 절감률 |",
    "| --- | --- |",
    "| 보고서 | 84% |",
  ].join("\n");

  it("splits markdown into slides", () => {
    const specs = markdownToSlideSpecs(deckMd);
    expect(specs.map((s) => s.title)).toEqual(["2026 AI 전략", "추진 배경", "기대 효과"]);
    expect(specs[1].notes).toEqual(["배경을 2분 안에 설명"]);
    expect(specs[1].blocks.filter((b) => b.type === "bullet")).toHaveLength(3);
    expect(markdownToSlideSpecs("# A\n\n본문\n\n---\n\n# B")).toHaveLength(2);
  });

  it("generates a deck that parses back with text, bullets, tables and notes", async () => {
    const bytes = await markdownToPptx(deckMd, "전략");
    const { deck } = await parsePptx(bytes);
    expect(deck.slides).toHaveLength(3);
    expect(deck.width).toBeGreaterThan(deck.height);
    const allText = (i: number) => deck.slides[i].shapes.flatMap((s) => s.paragraphs.map((p) => p.text)).join("|");
    expect(allText(0)).toContain("2026 AI 전략");
    expect(allText(1)).toContain("한국어 성능 우수");
    expect(allText(1)).toContain("512K 컨텍스트");
    const bulletPara = deck.slides[1].shapes.flatMap((s) => s.paragraphs).find((p) => p.text === "512K 컨텍스트");
    expect(bulletPara?.level).toBe(1);
    expect(bulletPara?.bullet).toBeTruthy();
    expect(deck.slides[1].notes).toContain("배경을 2분 안에 설명");
    const table = deck.slides[2].shapes.find((s) => s.type === "table");
    expect(table?.table?.rows[1]).toEqual(["보고서", "84%"]);

    const md = deckToMarkdown(deck);
    expect(md).toContain("## 슬라이드 2");
    expect(md).toContain("- 한국어 성능 우수");
    expect(md).toContain("| 보고서 | 84% |");
  });

  it("applies canvas edits in place and keeps run formatting", async () => {
    const bytes = await markdownToPptx(deckMd, "전략");
    const session = await parsePptx(bytes);
    const para = session.deck.slides[1].shapes.flatMap((s) => s.paragraphs).find((p) => p.text.startsWith("한국어"))!;
    const size = para.runs[0].sizePt;
    const edited = await applyPptxEdits(session, new Map([[para.key, "한국어 성능 최상위"]]));
    const again = await parsePptx(edited);
    const p2 = again.deck.slides[1].shapes.flatMap((s) => s.paragraphs).find((p) => p.text.startsWith("한국어"))!;
    expect(p2.text).toBe("한국어 성능 최상위");
    expect(p2.runs[0].sizePt).toBe(size);
    expect(again.deck.slides).toHaveLength(3);
  });

  it("replaces text in slides through the bridge, retrying without markdown symbols", async () => {
    const bytes = await markdownToPptx(deckMd, "전략");
    const reply = await handleDocBridgeRequest({
      requestId: "1",
      op: "replace",
      kind: "pptx",
      dataBase64: bytesToBase64(bytes),
      target: "- 도구 호출 지원",
      replacement: "- **도구 호출** 완벽 지원",
    });
    expect(reply.ok).toBe(true);
    const md = deckToMarkdown((await parsePptx(base64ToBytes(reply.dataBase64!))).deck);
    expect(md).toContain("도구 호출 완벽 지원");
  });

  it("bridge extracts and generates documents", async () => {
    const gen = await handleDocBridgeRequest({ requestId: "2", op: "generate", kind: "docx", markdown: "# 제목\n\n본문", title: "t" });
    expect(gen.ok).toBe(true);
    const ext = await handleDocBridgeRequest({ requestId: "3", op: "extract", kind: "docx", dataBase64: gen.dataBase64 });
    expect(ext.text).toContain("# 제목");
    const bad = await handleDocBridgeRequest({ requestId: "4", op: "generate", kind: "pdf", markdown: "x" });
    expect(bad.ok).toBe(false);
  });
});

/** 텍스트 한 줄짜리 최소 PDF를 만든다 (xref 오프셋 계산 포함). */
function minimalPdf(lines: string[]): Uint8Array {
  const content = lines.map((l, i) => `BT /F1 18 Tf 72 ${720 - i * 30} Td (${l}) Tj ET`).join("\n");
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
    `<< /Length ${content.length} >>\nstream\n${content}\nendstream`,
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
  ];
  let pdf = "%PDF-1.4\n";
  const offsets: number[] = [];
  objects.forEach((o, i) => {
    offsets.push(pdf.length);
    pdf += `${i + 1} 0 obj\n${o}\nendobj\n`;
  });
  const xref = pdf.length;
  pdf += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  offsets.forEach((o) => (pdf += `${String(o).padStart(10, "0")} 00000 n \n`));
  pdf += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return new TextEncoder().encode(pdf);
}

describe("pdf", () => {
  it("extracts text per page", async () => {
    const pkg = dirname(require.resolve("pdfjs-dist/package.json"));
    configurePdfjs({
      workerSrc: pathToFileURL(join(pkg, "build", "pdf.worker.mjs")).href,
      assetBase: join(pkg, "/"),
    });
    const text = await extractPdfText(minimalPdf(["Hello Any LM", "Second line"]));
    expect(text).toContain("## 페이지 1");
    expect(text).toContain("Hello Any LM");
    expect(text).toContain("Second line");
  });
});

describe("markdownToHtml", () => {
  it("renders GFM tables", () => {
    expect(markdownToHtml("| a | b |\n|---|---|\n| 1 | 2 |")).toContain("<table>");
  });
});
