import createDOMPurify, { type DOMPurify } from "dompurify";
import { marked } from "marked";

/**
 * 모델 답변·문서에서 나온 HTML을 화면에 넣기 전에 거른다.
 * 스크립트·이벤트 처리기·폼·외부 리소스를 없애, 프롬프트 인젝션으로 들어온 HTML이
 * 앱의 Tauri 명령(파일 쓰기, 셸 실행 등)에 손대지 못하게 한다.
 */

const SAFE_LINK = /^(?:https?:|mailto:)/i;
// 외부 이미지는 사용자 모르게 요청을 보내는 데(정보 유출·추적) 쓰일 수 있어 내장 이미지만 허용한다.
const SAFE_IMAGE = /^data:image\/(?:png|jpe?g|gif|webp|bmp);base64,/i;

let purifier: DOMPurify | null = null;

function getPurifier(): DOMPurify {
  if (purifier) return purifier;
  const p = createDOMPurify(window);
  p.addHook("afterSanitizeAttributes", (node) => {
    const el = node as Element;
    if (el.tagName === "A") {
      const href = (el.getAttribute("href") || "").trim();
      if (!SAFE_LINK.test(href)) el.removeAttribute("href");
      el.removeAttribute("target");
      el.setAttribute("rel", "noopener noreferrer");
    } else if (el.tagName === "IMG") {
      const src = (el.getAttribute("src") || "").trim();
      if (!SAFE_IMAGE.test(src)) el.removeAttribute("src");
    }
  });
  purifier = p;
  return p;
}

export function sanitizeHtml(html: string): string {
  return getPurifier().sanitize(html, {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ["style", "form", "button", "textarea", "select", "option", "iframe", "frame", "object", "embed", "link", "meta", "base", "audio", "video", "source", "track", "svg", "math", "template"],
    FORBID_ATTR: ["style", "srcset", "action", "formaction", "ping", "background", "poster"],
    ALLOW_DATA_ATTR: false,
  });
}

/** 채팅 답변 Markdown → 안전한 HTML */
export function renderMarkdownSafe(md: string): string {
  return sanitizeHtml(marked.parse(md, { gfm: true, async: false }) as string);
}

/** http(s)·mailto 링크인지 (외부 브라우저로 열어도 되는지) */
export function isExternalLink(href: string | null | undefined): href is string {
  return !!href && SAFE_LINK.test(href.trim());
}
