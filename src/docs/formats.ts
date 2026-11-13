/** 캔버스와 AI 도구가 다루는 문서 종류 */
export type DocKind = "markdown" | "docx" | "pptx" | "pdf" | "html" | "text";

/** 바이너리로 읽고 써야 하는 문서 종류 */
export const BINARY_KINDS: readonly DocKind[] = ["docx", "pptx", "pdf"];

export function extensionOf(path: string): string {
  const name = path.split(/[\\/]/).pop() || "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

export function docKindOf(path: string): DocKind {
  switch (extensionOf(path)) {
    case "md":
    case "markdown":
      return "markdown";
    case "docx":
      return "docx";
    case "pptx":
      return "pptx";
    case "pdf":
      return "pdf";
    case "html":
    case "htm":
      return "html";
    default:
      return "text";
  }
}

/** 코드 블록 언어 이름으로 문서 종류를 추정한다 (채팅 메시지 아티팩트용). */
export function docKindOfLanguage(lang: string): DocKind {
  const l = lang.toLowerCase();
  if (l === "md" || l === "markdown") return "markdown";
  if (l === "html" || l === "htm") return "html";
  return "text";
}

export function isBinaryKind(kind: DocKind): boolean {
  return BINARY_KINDS.includes(kind);
}

export const KIND_LABEL: Record<DocKind, string> = {
  markdown: "Markdown",
  docx: "Word 문서",
  pptx: "PowerPoint",
  pdf: "PDF",
  html: "HTML",
  text: "텍스트",
};

export const KIND_EXTENSION: Record<DocKind, string> = {
  markdown: "md",
  docx: "docx",
  pptx: "pptx",
  pdf: "pdf",
  html: "html",
  text: "txt",
};

/** 각 문서 종류에서 "다른 이름으로 저장" 시 고를 수 있는 형식 (첫 번째가 기본값) */
export const SAVE_AS_TARGETS: Record<DocKind, DocKind[]> = {
  markdown: ["markdown", "docx", "pptx", "html"],
  docx: ["docx", "markdown", "html"],
  pptx: ["pptx", "markdown"],
  pdf: ["pdf", "markdown"],
  html: ["html", "markdown", "docx"],
  text: ["text"],
};

export function replaceExtension(fileName: string, kind: DocKind): string {
  const base = fileName.replace(/\.[^./\\]+$/, "");
  return `${base || "문서"}.${KIND_EXTENSION[kind]}`;
}
