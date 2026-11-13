import { marked } from "marked";
import TurndownService from "turndown";
import { gfm } from "turndown-plugin-gfm";
import { sanitizeHtml } from "../security/sanitize";

export function markdownToHtml(md: string): string {
  // Markdown 안의 원시 HTML(스크립트·이벤트 처리기 등)은 편집기에 넣기 전에 걸러 낸다.
  return sanitizeHtml(marked.parse(md, { gfm: true, breaks: false, async: false }) as string);
}

function createTurndown(): TurndownService {
  const td = new TurndownService({
    headingStyle: "atx",
    codeBlockStyle: "fenced",
    bulletListMarker: "-",
    emDelimiter: "*",
    strongDelimiter: "**",
  });
  td.use(gfm);
  // 밑줄은 Markdown 문법이 없으므로 HTML 태그로 남긴다.
  td.keep(["u"]);
  // 편집기는 목록 항목 안에 <p>를 넣기 때문에 기본 규칙은 항목 사이에 빈 줄과 넓은 들여쓰기를 만든다.
  // "- 항목" 형태의 촘촘한 목록으로 출력한다.
  td.addRule("tightListItem", {
    filter: "li",
    replacement(content, node, options) {
      const parent = node.parentNode as HTMLElement | null;
      let prefix = `${options.bulletListMarker} `;
      if (parent?.nodeName === "OL") {
        const start = Number(parent.getAttribute("start") || 1);
        prefix = `${start + Array.prototype.indexOf.call(parent.children, node)}. `;
      }
      const indent = " ".repeat(prefix.length);
      const body = content
        .replace(/^\n+/, "")
        .replace(/\n+$/, "")
        .replace(/\n[ \t]*\n/g, "\n")
        .replace(/\n/g, `\n${indent}`);
      return prefix + body + (node.nextSibling ? "\n" : "");
    },
  });
  return td;
}

/**
 * 편집기 HTML을 Markdown으로 바꾼다.
 * TipTap 표는 셀 안에 <p>를 넣고 <colgroup>을 붙이므로, GFM 표가 깨지지 않도록 먼저 정리한다.
 */
export function htmlToMarkdown(html: string): string {
  const doc = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
  doc.querySelectorAll("colgroup").forEach((c) => c.remove());
  doc.querySelectorAll("td, th").forEach((cell) => {
    const paragraphs = Array.from(cell.children).filter((c) => c.tagName === "P");
    if (paragraphs.length > 0) {
      cell.innerHTML = paragraphs.map((p) => p.innerHTML).join(" ");
    }
  });
  doc.querySelectorAll("table").forEach((table) => {
    const first = table.querySelector("tr");
    if (first && !first.querySelector("th")) {
      first.querySelectorAll("td").forEach((td) => {
        const th = doc.createElement("th");
        th.innerHTML = td.innerHTML;
        td.replaceWith(th);
      });
    }
  });
  const md = createTurndown().turndown(doc.body.innerHTML).trim();
  return md ? md + "\n" : "";
}

/** 인라인 Markdown 기호(굵게, 기울임, 코드, 제목/목록 접두어)를 제거한 평문을 돌려준다. */
export function stripInlineMarkdown(text: string): string {
  return text
    .split("\n")
    .map((line) =>
      line
        .replace(/^\s{0,3}#{1,6}\s+/, "")
        .replace(/^\s*(?:[-*+]|\d+[.)])\s+/, "")
        .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
        .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
        .replace(/(\*\*|__)(.+?)\1/g, "$2")
        .replace(/(\*|_)(.+?)\1/g, "$2")
        .replace(/~~(.+?)~~/g, "$1")
        .replace(/`([^`]+)`/g, "$1")
        .replace(/<\/?u>/g, "")
    )
    .join("\n");
}

export function wrapHtmlDocument(bodyHtml: string, title: string): string {
  const safeTitle = title.replace(/[<>&"]/g, "");
  return `<!doctype html>
<html lang="ko">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${safeTitle}</title>
<style>
body { font-family: "Malgun Gothic", "Apple SD Gothic Neo", sans-serif; max-width: 860px; margin: 40px auto; padding: 0 20px; line-height: 1.7; color: #1f1e1d; }
table { border-collapse: collapse; } th, td { border: 1px solid #ccc; padding: 6px 10px; }
pre { background: #f5f5f4; padding: 12px; overflow-x: auto; } blockquote { border-left: 4px solid #ddd; margin: 0; padding-left: 16px; color: #555; }
</style>
</head>
<body>
${bodyHtml}
</body>
</html>
`;
}

/** 완전한 HTML 문서에서 <body> 안쪽만 꺼낸다. */
export function extractBodyHtml(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  return doc.body ? doc.body.innerHTML : html;
}
