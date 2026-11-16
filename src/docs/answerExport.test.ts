// @vitest-environment jsdom
import JSZip from "jszip";
import { describe, expect, it } from "vitest";
import { docxToMarkdown } from "./docx";
import { hwpxToMarkdown } from "./hwpx";
import { exportAnswer, parentDir, sanitizeFileName, suggestAnswerFileName } from "./answerExport";

const ANSWER = `## 삼성전자 2025년 실적 요약

2025년 연간 **매출은 333.61조 원**입니다.

| 분기 | 매출(조 원) | 영업이익(조 원) |
| --- | --- | --- |
| 1분기 | 79.14 | 6.69 |
| 4분기 | 93.84 | 20.07 |

- 메모리 회복
- 파운드리 적자 축소
`;

describe("답변 파일 이름", () => {
  it("답변의 첫 제목을 쓴다", () => {
    expect(suggestAnswerFileName(ANSWER, "실적 알려줘")).toBe("삼성전자 2025년 실적 요약");
  });

  it("제목이 없으면 질문 첫 줄, 그것도 없으면 날짜", () => {
    expect(suggestAnswerFileName("그냥 본문입니다.", "**보고서** 써줘\n자세히")).toBe("보고서 써줘");
    expect(suggestAnswerFileName("본문", undefined, new Date(2026, 10, 4))).toBe("엑사원 답변 2026-11-04");
  });

  it("Windows에서 못 쓰는 글자와 이름을 걸러 낸다", () => {
    expect(sanitizeFileName('A/B: "견적"? <초안>|*.')).toBe("A B 견적 초안");
    expect(sanitizeFileName("con")).toBe("");
    expect(sanitizeFileName("가".repeat(80))).toHaveLength(50);
    expect(suggestAnswerFileName("# ...", "???")).toMatch(/^엑사원 답변 \d{4}-\d\d-\d\d$/);
  });

  it("고른 경로의 폴더를 기억한다", () => {
    expect(parentDir("C:\\DOCS\\TEST\\a.md")).toBe("C:\\DOCS\\TEST");
    expect(parentDir("a.md")).toBeUndefined();
  });
});

describe("답변 내보내기", () => {
  it("Markdown은 본문 그대로 UTF-8로 저장한다", async () => {
    const bytes = await exportAnswer(`\n${ANSWER}\n\n`, "markdown", "x");
    const text = new TextDecoder().decode(bytes);
    expect(text).toBe(ANSWER.trim() + "\n");
    expect(bytes[0]).not.toBe(0xef); // BOM 없음
  });

  it("Word 문서는 제목·표·굵게·목록을 살린다", async () => {
    const bytes = await exportAnswer(ANSWER, "docx", "삼성전자 2025년 실적 요약");
    const zip = await JSZip.loadAsync(bytes);
    const xml = await zip.file("word/document.xml")!.async("string");
    expect(xml).toContain("<w:tbl>");
    expect(xml).toContain("Heading2");
    const core = await zip.file("docProps/core.xml")!.async("string");
    expect(core).toContain("삼성전자 2025년 실적 요약");

    const back = await docxToMarkdown(bytes);
    expect(back).toContain("삼성전자 2025년 실적 요약");
    expect(back).toContain("**매출은 333.61조 원**");
    expect(back).toMatch(/\|\s*4분기\s*\|\s*93\.84\s*\|\s*20\.07\s*\|/);
    expect(back).toMatch(/-\s+메모리 회복/);
  });

  it("한글 문서도 제목·표·굵게·목록을 살린다", async () => {
    const bytes = await exportAnswer(ANSWER, "hwpx", "삼성전자 2025년 실적 요약");
    const zip = await JSZip.loadAsync(bytes);
    expect(Object.keys(zip.files)[0]).toBe("mimetype");
    expect(await zip.file("Contents/content.hpf")!.async("string")).toContain("삼성전자 2025년 실적 요약");
    const back = await hwpxToMarkdown(bytes);
    expect(back).toContain("## 삼성전자 2025년 실적 요약");
    expect(back).toContain("**매출은 333.61조 원**");
    expect(back).toMatch(/\|\s*4분기\s*\|\s*93\.84\s*\|\s*20\.07\s*\|/);
    expect(back).toMatch(/-\s+메모리 회복/);
  });
});
