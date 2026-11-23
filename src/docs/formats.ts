/** 캔버스와 AI 도구가 다루는 문서 종류 */
export type DocKind = "markdown" | "docx" | "hwpx" | "pptx" | "xlsx" | "pdf" | "html" | "image" | "text";

/** 바이너리로 읽고 써야 하는 문서 종류 */
export const BINARY_KINDS: readonly DocKind[] = ["docx", "hwpx", "pptx", "xlsx", "pdf", "image"];

/** 캔버스에서 미리보기만 하는 문서 종류 (고치기는 원래 앱이나 모델에게 맡긴다) */
export const PREVIEW_ONLY_KINDS: readonly DocKind[] = ["pptx", "xlsx", "pdf", "image"];

export function isPreviewOnlyKind(kind: DocKind): boolean {
  return PREVIEW_ONLY_KINDS.includes(kind);
}

/** 캔버스로 열 수 없어 Windows 기본 앱(엑셀 등)으로 여는 파일 */
const EXTERNAL_APP_EXTENSIONS = new Set([
  "xls", "doc", "ppt", "hwp",
  "mp3", "wav", "mp4", "zip",
]);

/** 캔버스 이미지 뷰어에서 미리보기로 여는 이미지 확장자 */
const IMAGE_EXTENSIONS = new Set(["png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "ico"]);

const IMAGE_MIME_TYPES: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  bmp: "image/bmp",
  webp: "image/webp",
  svg: "image/svg+xml",
  ico: "image/x-icon",
};

export function imageMimeType(path: string): string {
  return IMAGE_MIME_TYPES[extensionOf(path)] ?? "application/octet-stream";
}

export function opensInExternalApp(path: string): boolean {
  return EXTERNAL_APP_EXTENSIONS.has(extensionOf(path));
}

export function isPythonScript(path: string): boolean {
  const ext = extensionOf(path);
  return ext === "py" || ext === "pyw";
}

export function extensionOf(path: string): string {
  const name = path.split(/[\\/]/).pop() || "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

export function docKindOf(path: string): DocKind {
  const ext = extensionOf(path);
  if (IMAGE_EXTENSIONS.has(ext)) return "image";
  switch (ext) {
    case "md":
    case "markdown":
      return "markdown";
    case "docx":
      return "docx";
    case "hwpx":
      return "hwpx";
    case "pptx":
      return "pptx";
    case "xlsx":
      return "xlsx";
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
  hwpx: "한글 문서",
  pptx: "PowerPoint",
  xlsx: "Excel",
  pdf: "PDF",
  html: "HTML",
  image: "이미지",
  text: "텍스트",
};

export const KIND_EXTENSION: Record<DocKind, string> = {
  markdown: "md",
  docx: "docx",
  hwpx: "hwpx",
  pptx: "pptx",
  xlsx: "xlsx",
  pdf: "pdf",
  html: "html",
  image: "png",
  text: "txt",
};

/** 각 문서 종류에서 "다른 이름으로 저장" 시 고를 수 있는 형식 (첫 번째가 기본값) */
export const SAVE_AS_TARGETS: Record<DocKind, DocKind[]> = {
  markdown: ["markdown", "docx", "hwpx", "pptx", "html"],
  docx: ["docx", "hwpx", "markdown", "html"],
  hwpx: ["hwpx", "docx", "markdown", "html"],
  pptx: ["pptx", "markdown"],
  xlsx: ["xlsx", "markdown"],
  pdf: ["pdf", "markdown"],
  html: ["html", "markdown", "docx", "hwpx"],
  image: ["image"],
  text: ["text"],
};

export function replaceExtension(fileName: string, kind: DocKind): string {
  const base = fileName.replace(/\.[^./\\]+$/, "");
  return `${base || "문서"}.${KIND_EXTENSION[kind]}`;
}
