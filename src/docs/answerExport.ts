import { markdownToDocx } from "./docx";
import { markdownToHwpx } from "./hwpx";
import { stripInlineMarkdown } from "./markdown";

/** 답변을 저장할 수 있는 형식 */
export type AnswerFormat = "markdown" | "docx" | "hwpx";

export const ANSWER_FORMATS: Record<AnswerFormat, { label: string; ext: string; filter: string; mime: string }> = {
  markdown: {
    label: "Markdown (.md)",
    ext: "md",
    filter: "Markdown 문서 (*.md)|*.md|모든 파일 (*.*)|*.*",
    mime: "text/markdown;charset=utf-8",
  },
  docx: {
    label: "Word 문서 (.docx)",
    ext: "docx",
    filter: "Word 문서 (*.docx)|*.docx|모든 파일 (*.*)|*.*",
    mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  },
  hwpx: {
    label: "한글 문서 (.hwpx)",
    ext: "hwpx",
    filter: "한글 문서 (*.hwpx)|*.hwpx|모든 파일 (*.*)|*.*",
    mime: "application/hwp+zip",
  },
};

const MAX_NAME_LENGTH = 50;
const WINDOWS_RESERVED = /^(con|prn|aux|nul|com\d|lpt\d)$/i;

/** Windows 파일 이름으로 쓸 수 없는 글자를 지우고 길이를 줄인다. 쓸 수 없으면 빈 문자열. */
export function sanitizeFileName(raw: string): string {
  let name = raw
    .replace(/[\u0000-\u001f\\/:*?"<>|]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  if (name.length > MAX_NAME_LENGTH) name = name.slice(0, MAX_NAME_LENGTH).trim();
  name = name.replace(/[.\s]+$/, "");
  if (WINDOWS_RESERVED.test(name)) return "";
  return name;
}

function firstMeaningfulLine(text: string): string {
  for (const line of stripInlineMarkdown(text).split("\n")) {
    const t = line.replace(/^[>|\s-]+/, "").trim();
    if (t && !t.startsWith("```")) return t;
  }
  return "";
}

/**
 * 저장 창에 미리 넣을 파일 이름(확장자 제외).
 * 답변의 첫 제목 → 사용자 질문 첫 줄 → "답변 날짜" 순으로 고른다.
 */
export function suggestAnswerFileName(answer: string, question?: string, now = new Date()): string {
  const heading = answer.match(/^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$/m)?.[1];
  for (const candidate of [heading, question ? firstMeaningfulLine(question) : ""]) {
    const name = candidate ? sanitizeFileName(stripInlineMarkdown(candidate)) : "";
    if (name) return name;
  }
  const pad = (n: number) => String(n).padStart(2, "0");
  return `답변 ${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

/** 답변 본문을 파일 내용으로 바꾼다. Markdown은 UTF-8 텍스트, Word·한글은 문서 바이트. */
export async function exportAnswer(answer: string, format: AnswerFormat, title: string): Promise<Uint8Array> {
  const md = answer.trim() + "\n";
  if (format === "docx") return markdownToDocx(md, title);
  if (format === "hwpx") return markdownToHwpx(md, title);
  return new TextEncoder().encode(md);
}

/** 저장 창에서 고른 경로의 폴더 (다음 저장 때 같은 폴더에서 시작) */
export function parentDir(path: string): string | undefined {
  const i = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return i > 0 ? path.slice(0, i) : undefined;
}
