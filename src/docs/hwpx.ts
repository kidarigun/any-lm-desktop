/**
 * 한글 HWPX(OWPML, KS X 6101) 문서 읽기·만들기·문구 바꾸기.
 *
 * HWPX는 ZIP 꾸러미다:
 *   mimetype (압축 없이 맨 앞, "application/hwp+zip")
 *   version.xml, settings.xml
 *   META-INF/container.xml, META-INF/manifest.xml, META-INF/container.rdf
 *   Contents/content.hpf (꾸러미 목록과 읽는 순서)
 *   Contents/header.xml (글꼴·글자 모양·문단 모양·스타일 목록)
 *   Contents/section0.xml … (본문. 문단 hp:p → 글자 묶음 hp:run → 글자 hp:t, 표 hp:tbl, 그림 hp:pic)
 *   BinData/… (그림 파일)
 *   Preview/PrvText.txt (미리보기 글)
 *
 * 읽기는 HTML(캔버스 편집기)과 Markdown(모델)으로, 만들기는 HTML·Markdown에서 한다.
 * 만든 문서는 한글 기본 글꼴(함초롬바탕·함초롬돋움)과 A4 용지를 쓴다.
 */
import JSZip from "jszip";
import { base64ToBytes, bytesToBase64 } from "./base64";
import { imageSize } from "./imageSize";
import { htmlToMarkdown, markdownToHtml } from "./markdown";

const HP_NS = "http://www.hancom.co.kr/hwpml/2011/paragraph";
const MIMETYPE = "application/hwp+zip";

// ---------------------------------------------------------------------------
// XML 도우미
// ---------------------------------------------------------------------------

function kids(el: Element | null | undefined, name: string): Element[] {
  if (!el) return [];
  return Array.from(el.children).filter((c) => c.localName === name);
}

function kid(el: Element | null | undefined, name: string): Element | null {
  return kids(el, name)[0] ?? null;
}

function descendants(el: Element | Document, name: string): Element[] {
  return Array.from(el.getElementsByTagName("*")).filter((c) => c.localName === name);
}

function parseXml(text: string): Document {
  const doc = new DOMParser().parseFromString(text, "application/xml");
  if (doc.getElementsByTagName("parsererror").length > 0) {
    throw new Error("HWPX 안의 XML을 해석할 수 없습니다. 파일이 손상되었을 수 있습니다.");
  }
  return doc;
}

function serializeXml(doc: Document): string {
  const body = new XMLSerializer().serializeToString(doc);
  return body.startsWith("<?xml") ? body : `<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>${body}`;
}

function esc(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    // XML 1.0에서 쓸 수 없는 제어 문자는 뺀다.
    // eslint-disable-next-line no-control-regex
    .replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F]/g, "");
}

function escHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function int(v: string | null | undefined, fallback = 0): number {
  const n = parseInt(v ?? "", 10);
  return Number.isFinite(n) ? n : fallback;
}

// ---------------------------------------------------------------------------
// 꾸러미 열기
// ---------------------------------------------------------------------------

interface ManifestItem {
  id: string;
  href: string;
  mediaType: string;
}

interface HwpxPackage {
  zip: JSZip;
  /** 본문 구역 파일 경로 (읽는 순서) */
  sections: string[];
  headerPath: string | null;
  items: Map<string, ManifestItem>;
}

function isCompoundFile(bytes: Uint8Array): boolean {
  return bytes.length > 8 && bytes[0] === 0xd0 && bytes[1] === 0xcf && bytes[2] === 0x11 && bytes[3] === 0xe0;
}

function sectionNumber(path: string): number {
  const m = /section(\d+)\.xml$/i.exec(path);
  return m ? Number(m[1]) : 0;
}

/** zip 안에서 href가 가리키는 파일을 찾는다 (꾸러미 기준 경로와 content.hpf 기준 경로 모두 시도). */
function resolveHref(zip: JSZip, href: string, baseDir: string): string | null {
  const clean = href.replace(/^\/+/, "");
  if (zip.file(clean)) return clean;
  const rel = baseDir ? `${baseDir}/${clean}` : clean;
  if (zip.file(rel)) return rel;
  const lower = clean.toLowerCase();
  const hit = Object.keys(zip.files).find((f) => f.toLowerCase() === lower || f.toLowerCase() === rel.toLowerCase());
  return hit ?? null;
}

async function openPackage(bytes: Uint8Array): Promise<HwpxPackage> {
  if (isCompoundFile(bytes)) {
    throw new Error(
      "HWP(한글 97~2018의 바이너리 형식) 파일은 열 수 없습니다. 한글에서 [다른 이름으로 저장] → HWPX 형식으로 저장한 뒤 여세요."
    );
  }
  let zip: JSZip;
  try {
    zip = await JSZip.loadAsync(bytes);
  } catch {
    throw new Error("HWPX 문서가 아닙니다 (ZIP 꾸러미를 열 수 없습니다).");
  }

  // content.hpf 위치는 container.xml이 알려 준다.
  let hpfPath = "Contents/content.hpf";
  const container = zip.file("META-INF/container.xml");
  if (container) {
    try {
      const doc = parseXml(await container.async("string"));
      const root = descendants(doc, "rootfile").find(
        (r) => (r.getAttribute("media-type") || "").includes("hwpml-package") || /\.hpf$/i.test(r.getAttribute("full-path") || "")
      );
      const p = root?.getAttribute("full-path");
      if (p && zip.file(p)) hpfPath = p;
    } catch {
      // container.xml이 깨져도 기본 위치로 계속한다.
    }
  }

  const items = new Map<string, ManifestItem>();
  let sections: string[] = [];
  let headerPath: string | null = null;
  const hpf = zip.file(hpfPath);
  if (hpf) {
    const baseDir = hpfPath.includes("/") ? hpfPath.slice(0, hpfPath.lastIndexOf("/")) : "";
    const doc = parseXml(await hpf.async("string"));
    for (const it of descendants(doc, "item")) {
      const id = it.getAttribute("id") || "";
      const href = resolveHref(zip, it.getAttribute("href") || "", baseDir);
      if (!id || !href) continue;
      items.set(id, { id, href, mediaType: it.getAttribute("media-type") || "" });
    }
    for (const ref of descendants(doc, "itemref")) {
      const item = items.get(ref.getAttribute("idref") || "");
      if (item && /section\d*\.xml$/i.test(item.href)) sections.push(item.href);
    }
    headerPath = items.get("header")?.href ?? null;
  }
  if (sections.length === 0) {
    sections = Object.keys(zip.files)
      .filter((f) => /^Contents\/section\d+\.xml$/i.test(f))
      .sort((a, b) => sectionNumber(a) - sectionNumber(b));
  }
  if (!headerPath && zip.file("Contents/header.xml")) headerPath = "Contents/header.xml";
  if (sections.length === 0) {
    throw new Error("HWPX 문서가 아닙니다 (본문 구역 Contents/section0.xml이 없습니다).");
  }
  return { zip, sections, headerPath, items };
}

// ---------------------------------------------------------------------------
// header.xml 해석
// ---------------------------------------------------------------------------

interface CharFmt {
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
  sup: boolean;
  sub: boolean;
}

interface ParaFmt {
  align: string;
  headingType: string;
  headingIdRef: string;
  level: number;
}

interface StyleInfo {
  name: string;
  engName: string;
}

interface HeaderInfo {
  chars: Map<string, CharFmt>;
  paras: Map<string, ParaFmt>;
  styles: Map<string, StyleInfo>;
  /** 번호 문단 정의 id → 수준별 번호 형식 (DIGIT, HANGUL_SYLLABLE …) */
  numberFormats: Map<string, string[]>;
}

function parseHeader(doc: Document | null): HeaderInfo {
  const info: HeaderInfo = { chars: new Map(), paras: new Map(), styles: new Map(), numberFormats: new Map() };
  if (!doc) return info;
  for (const c of descendants(doc, "charPr")) {
    const underline = kid(c, "underline");
    const strike = kid(c, "strikeout");
    info.chars.set(c.getAttribute("id") || "", {
      bold: !!kid(c, "bold"),
      italic: !!kid(c, "italic"),
      underline: !!underline && (underline.getAttribute("type") || "NONE") !== "NONE",
      strike: !!strike && !["NONE", ""].includes(strike.getAttribute("shape") || "NONE"),
      sup: !!kid(c, "supscript"),
      sub: !!kid(c, "subscript"),
    });
  }
  for (const p of descendants(doc, "paraPr")) {
    const heading = kid(p, "heading");
    info.paras.set(p.getAttribute("id") || "", {
      align: kid(p, "align")?.getAttribute("horizontal") || "JUSTIFY",
      headingType: heading?.getAttribute("type") || "NONE",
      headingIdRef: heading?.getAttribute("idRef") || "0",
      level: int(heading?.getAttribute("level")),
    });
  }
  for (const s of descendants(doc, "style")) {
    info.styles.set(s.getAttribute("id") || "", {
      name: s.getAttribute("name") || "",
      engName: s.getAttribute("engName") || "",
    });
  }
  for (const n of descendants(doc, "numbering")) {
    const formats: string[] = [];
    for (const h of kids(n, "paraHead")) formats[Math.max(0, int(h.getAttribute("level"), 1) - 1)] = h.getAttribute("numFormat") || "DIGIT";
    info.numberFormats.set(n.getAttribute("id") || "", formats);
  }
  return info;
}

/** 스타일 이름으로 제목 수준을 알아낸다 ("개요 1", "제목 2", "Heading 3", "Outline 1"). */
export function headingLevelOfStyle(style: StyleInfo | undefined): number | null {
  if (!style) return null;
  const m =
    /^(?:개요|제목)\s*(\d+)$/.exec(style.name.trim()) || /^(?:outline|heading)\s*(\d+)$/i.exec(style.engName.trim());
  if (!m) return null;
  const n = Number(m[1]);
  return n >= 1 ? Math.min(n, 6) : null;
}

// ---------------------------------------------------------------------------
// HWPX → HTML
// ---------------------------------------------------------------------------

export interface HwpxReadOptions {
  /** 그림을 data URL로 넣을지(캔버스), 파일 이름만 남길지(모델에게 보낼 글) */
  images?: "data" | "placeholder";
}

interface ReadContext {
  pkg: HwpxPackage;
  header: HeaderInfo;
  images: "data" | "placeholder";
  /** binaryItemIDRef → data URL (한 번만 읽는다) */
  imageCache: Map<string, string>;
  notes: string[];
  /** 제목·표 머리글 칸처럼 굵은 글씨가 기본인 곳에서는 굵게 표시를 빼고 읽는다 */
  boldIsDefault?: boolean;
}

type Block =
  | { type: "html"; html: string }
  | { type: "li"; ordered: boolean; level: number; html: string; listId: string };

const ALIGN_CSS: Record<string, string> = { CENTER: "center", RIGHT: "right" };

function wrapFormat(html: string, fmt: CharFmt | undefined, boldIsDefault = false): string {
  if (!fmt || !html) return html;
  let out = html;
  if (fmt.sup) out = `<sup>${out}</sup>`;
  if (fmt.sub) out = `<sub>${out}</sub>`;
  if (fmt.strike) out = `<s>${out}</s>`;
  if (fmt.underline) out = `<u>${out}</u>`;
  if (fmt.italic) out = `<em>${out}</em>`;
  if (fmt.bold && !boldIsDefault) out = `<strong>${out}</strong>`;
  return out;
}

/** hp:t 안의 글자. 탭·줄바꿈 같은 요소가 글자 사이에 섞여 있다. */
function textElementHtml(t: Element): string {
  let out = "";
  t.childNodes.forEach((n) => {
    if (n.nodeType === 3 || n.nodeType === 4) {
      out += escHtml(n.nodeValue || "");
      return;
    }
    if (n.nodeType !== 1) return;
    switch ((n as Element).localName) {
      case "lineBreak":
        out += "<br>";
        break;
      case "tab":
        out += "\t";
        break;
      case "nbSpace":
      case "fwSpace":
        out += " ";
        break;
      case "hyphen":
        out += "-";
        break;
      default:
        // 형광펜·변경 추적 표시 같은 요소는 글자가 없다.
        out += escHtml((n as Element).textContent || "");
    }
  });
  return out;
}

async function imageHtml(pic: Element, ctx: ReadContext): Promise<string> {
  const img = descendants(pic, "img")[0];
  const ref = img?.getAttribute("binaryItemIDRef") || "";
  const item = ctx.pkg.items.get(ref);
  const name = item?.href.split("/").pop() || ref || "그림";
  if (ctx.images === "placeholder" || !item) return `<img alt="그림" src="${escHtml(name)}">`;
  let src = ctx.imageCache.get(ref);
  if (!src) {
    const file = ctx.pkg.zip.file(item.href);
    if (!file) return `<img alt="그림" src="${escHtml(name)}">`;
    const ext = (item.href.split(".").pop() || "png").toLowerCase();
    const mime = item.mediaType.startsWith("image/") ? item.mediaType : `image/${ext === "jpg" ? "jpeg" : ext}`;
    src = `data:${mime};base64,${bytesToBase64(await file.async("uint8array"))}`;
    ctx.imageCache.set(ref, src);
  }
  // 문서에 놓인 크기(HWPUNIT, 1px = 75)로 보여 준다.
  const cur = kid(pic, "curSz") || kid(pic, "sz");
  const w = Math.round(int(cur?.getAttribute("width")) / 75);
  return `<img src="${src}" alt="그림"${w > 0 ? ` width="${Math.min(w, 640)}"` : ""}>`;
}

async function subListBlocks(container: Element, ctx: ReadContext): Promise<Block[]> {
  const out: Block[] = [];
  for (const p of kids(container, "p")) out.push(...(await paragraphBlocks(p, ctx)));
  return out;
}

async function cellHtml(sub: Element | null, ctx: ReadContext): Promise<string> {
  if (!sub) return "";
  const blocks = await subListBlocks(sub, ctx);
  return blocksToHtml(blocks);
}

async function tableHtml(tbl: Element, ctx: ReadContext): Promise<string> {
  const rows: string[] = [];
  for (const tr of kids(tbl, "tr")) {
    const cells: string[] = [];
    for (const tc of kids(tr, "tc")) {
      const span = kid(tc, "cellSpan");
      const colSpan = int(span?.getAttribute("colSpan"), 1);
      const rowSpan = int(span?.getAttribute("rowSpan"), 1);
      const tag = tc.getAttribute("header") === "1" ? "th" : "td";
      const attrs = `${colSpan > 1 ? ` colspan="${colSpan}"` : ""}${rowSpan > 1 ? ` rowspan="${rowSpan}"` : ""}`;
      const inner = await cellHtml(kid(tc, "subList"), tag === "th" ? { ...ctx, boldIsDefault: true } : ctx);
      cells.push(`<${tag}${attrs}>${inner || "<p></p>"}</${tag}>`);
    }
    rows.push(`<tr>${cells.join("")}</tr>`);
  }
  return `<table><tbody>${rows.join("")}</tbody></table>`;
}

/** 글상자·도형 안의 글 (hp:drawText 안의 문단 목록) */
async function drawTextBlocks(shape: Element, ctx: ReadContext): Promise<Block[]> {
  const out: Block[] = [];
  for (const dt of descendants(shape, "drawText")) {
    const sub = kid(dt, "subList");
    if (sub) out.push(...(await subListBlocks(sub, ctx)));
  }
  return out;
}

const SHAPE_ELEMENTS = new Set(["rect", "ellipse", "arc", "polygon", "curve", "connectLine", "container", "line"]);

async function paragraphBlocks(p: Element, ctx: ReadContext): Promise<Block[]> {
  const para = ctx.header.paras.get(p.getAttribute("paraPrIDRef") || "");
  const style = ctx.header.styles.get(p.getAttribute("styleIDRef") || "");
  const level = headingLevelOfStyle(style) ?? (para?.headingType === "OUTLINE" ? Math.min(para.level + 1, 6) : null);
  const boldIsDefault = !!level || !!ctx.boldIsDefault;
  let inline = "";
  const after: Block[] = [];

  for (const run of kids(p, "run")) {
    const fmt = ctx.header.chars.get(run.getAttribute("charPrIDRef") || "");
    for (const child of Array.from(run.children)) {
      switch (child.localName) {
        case "t":
          inline += wrapFormat(textElementHtml(child), fmt, boldIsDefault);
          break;
        case "tbl":
          after.push({ type: "html", html: await tableHtml(child, ctx) });
          break;
        case "pic":
          inline += await imageHtml(child, ctx);
          break;
        case "equation": {
          const script = kid(child, "script")?.textContent?.trim();
          if (script) inline += `<code>${escHtml(script)}</code>`;
          break;
        }
        case "ctrl":
          for (const c of Array.from(child.children)) {
            if (c.localName === "footNote" || c.localName === "endNote") {
              const sub = kid(c, "subList");
              const text = sub
                ? (await subListBlocks(sub, ctx))
                    .map((b) => b.html.replace(/<[^>]+>/g, ""))
                    .join(" ")
                    .trim()
                : "";
              ctx.notes.push(text);
              inline += `<sup>[${ctx.notes.length}]</sup>`;
            }
          }
          break;
        default:
          if (SHAPE_ELEMENTS.has(child.localName)) after.push(...(await drawTextBlocks(child, ctx)));
      }
    }
  }

  // 같은 모양의 글자 묶음이 이어지면 하나로 합친다 ("**가****나**" 대신 "**가나**").
  inline = inline.replace(/<\/(strong|em|u|s|sup|sub)><\1>/g, "");
  const blocks: Block[] = [];
  const align = ALIGN_CSS[para?.align || ""];
  const styleAttr = align ? ` style="text-align: ${align}"` : "";
  const hasText = inline.replace(/<[^>]+>/g, "").trim() !== "" || /<img/.test(inline);

  if (level && hasText) {
    blocks.push({ type: "html", html: `<h${level}${styleAttr}>${inline}</h${level}>` });
  } else if ((para?.headingType === "BULLET" || para?.headingType === "NUMBER") && hasText) {
    blocks.push({
      type: "li",
      ordered: para.headingType === "NUMBER",
      level: Math.max(0, para.level),
      html: inline,
      listId: `${para.headingType}:${para.headingIdRef}`,
    });
  } else if (hasText || after.length === 0) {
    // 빈 문단도 문서의 줄 간격을 이루므로 남긴다 (표만 든 문단은 표만).
    blocks.push({ type: "html", html: `<p${styleAttr}>${inline}</p>` });
  }
  blocks.push(...after);
  return blocks;
}

/** 이어진 목록 문단을 <ul>/<ol>로 묶는다. */
function blocksToHtml(blocks: Block[]): string {
  let html = "";
  let i = 0;
  while (i < blocks.length) {
    const b = blocks[i];
    if (b.type === "html") {
      html += b.html;
      i++;
      continue;
    }
    // 목록: 같은 연속 구간을 수준에 맞게 중첩한다.
    const stack: { tag: string; level: number }[] = [];
    while (i < blocks.length && blocks[i].type === "li") {
      const li = blocks[i] as Extract<Block, { type: "li" }>;
      const tag = li.ordered ? "ol" : "ul";
      while (stack.length > 0 && stack[stack.length - 1].level > li.level) {
        html += `</li></${stack.pop()!.tag}>`;
      }
      const top = stack[stack.length - 1];
      if (!top || top.level < li.level) {
        html += `<${tag}><li>`;
        stack.push({ tag, level: li.level });
      } else if (top.tag !== tag) {
        html += `</li></${stack.pop()!.tag}><${tag}><li>`;
        stack.push({ tag, level: li.level });
      } else {
        html += "</li><li>";
      }
      html += `<p>${li.html}</p>`;
      i++;
    }
    while (stack.length > 0) html += `</li></${stack.pop()!.tag}>`;
  }
  return html;
}

export async function hwpxToHtml(bytes: Uint8Array, opts: HwpxReadOptions = {}): Promise<string> {
  const pkg = await openPackage(bytes);
  const headerFile = pkg.headerPath ? pkg.zip.file(pkg.headerPath) : null;
  const header = parseHeader(headerFile ? parseXml(await headerFile.async("string")) : null);
  const ctx: ReadContext = { pkg, header, images: opts.images ?? "data", imageCache: new Map(), notes: [] };
  const blocks: Block[] = [];
  for (const path of pkg.sections) {
    const file = pkg.zip.file(path);
    if (!file) continue;
    const sec = parseXml(await file.async("string")).documentElement;
    for (const p of kids(sec, "p")) blocks.push(...(await paragraphBlocks(p, ctx)));
  }
  // 문서 처음과 끝의 빈 문단은 뺀다 (첫 문단은 구역 설정만 든 경우가 많다).
  const isEmpty = (b: Block | undefined) => !!b && b.type === "html" && /^<p[^>]*><\/p>$/.test(b.html);
  while (isEmpty(blocks[blocks.length - 1])) blocks.pop();
  while (isEmpty(blocks[0])) blocks.shift();
  let html = blocksToHtml(blocks);
  if (ctx.notes.length > 0) {
    html += "<hr>" + ctx.notes.map((n, i) => `<p>[${i + 1}] ${escHtml(n)}</p>`).join("");
  }
  return html;
}

/** 모델에게 보낼 Markdown. 그림은 파일 이름만 남긴다. */
export async function hwpxToMarkdown(bytes: Uint8Array): Promise<string> {
  return compactMarkdownTables(htmlToMarkdown(await hwpxToHtml(bytes, { images: "placeholder" })));
}

/**
 * 모양을 내려고 쓴 표(순서도·제목 상자)는 빈 칸이 대부분이다.
 * Markdown 표에서 모두 빈 열과 빈 행을 빼서 읽기 쉽게 한다.
 */
export function compactMarkdownTables(md: string): string {
  const lines = md.split("\n");
  const out: string[] = [];
  const splitRow = (line: string) =>
    line
      .trim()
      .replace(/^\||\|$/g, "")
      .split(/(?<!\\)\|/)
      .map((c) => c.trim());
  let i = 0;
  while (i < lines.length) {
    if (!lines[i].trimStart().startsWith("|")) {
      out.push(lines[i++]);
      continue;
    }
    const block: string[] = [];
    while (i < lines.length && lines[i].trimStart().startsWith("|")) block.push(lines[i++]);
    const rows = block.map(splitRow);
    const sepIndex = rows.findIndex((r) => r.length > 0 && r.every((c) => /^:?-{3,}:?$/.test(c)));
    const width = Math.max(...rows.map((r) => r.length));
    const isEmpty = (c: string | undefined) => !c || c.replace(/\*\*|__|\*|_/g, "").trim() === "";
    const keep = Array.from({ length: width }, (_, col) =>
      rows.some((r, ri) => ri !== sepIndex && !isEmpty(r[col]))
    );
    if (!keep.some(Boolean)) continue; // 모두 빈 표는 뺀다
    const dataRows = rows.filter((r, ri) => ri === sepIndex || r.some((c, col) => keep[col] && !isEmpty(c)));
    // 머리글 줄이 비면 첫 데이터 줄을 머리글로 올린다
    if (sepIndex === 1 && dataRows[0] !== rows[0] && dataRows.length >= 2) {
      const sep = dataRows.splice(dataRows.indexOf(rows[1]), 1)[0];
      dataRows.splice(1, 0, sep);
    }
    const kept = keep.filter(Boolean).length;
    for (const r of dataRows) {
      if (sepIndex >= 0 && r === rows[sepIndex]) {
        out.push(`| ${Array(kept).fill("---").join(" | ")} |`);
        continue;
      }
      const cells = keep.map((k, col) => (k ? (r[col] ?? "") : null)).filter((c): c is string => c !== null);
      out.push(`| ${cells.join(" | ")} |`);
    }
  }
  return out.join("\n");
}

// ---------------------------------------------------------------------------
// HTML → HWPX
// ---------------------------------------------------------------------------

/** 용지 설정 (HWPUNIT: 1mm ≈ 283.46, 1pt = 100) */
export interface PageSetup {
  width: number;
  height: number;
  landscape: "WIDELY" | "NARROWLY";
  left: number;
  right: number;
  top: number;
  bottom: number;
  header: number;
  footer: number;
  gutter: number;
}

/** A4 세로, 한글 기본 여백 (왼쪽·오른쪽 30mm, 위 20mm, 아래 15mm, 머리말·꼬리말 15mm) */
export const A4_PAGE: PageSetup = {
  width: 59528,
  height: 84186,
  landscape: "WIDELY",
  left: 8504,
  right: 8504,
  top: 5668,
  bottom: 4252,
  header: 4252,
  footer: 4252,
  gutter: 0,
};

interface RunFmt {
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strike?: boolean;
  sup?: boolean;
  sub?: boolean;
  code?: boolean;
  link?: boolean;
  /** 글자 크기 (pt × 100) */
  size?: number;
  /** 제목용 돋움 글꼴 */
  gothic?: boolean;
}

type WRun = { kind: "text"; text: string; fmt: RunFmt } | { kind: "br" } | { kind: "img"; ref: ImageRef };

interface ImageRef {
  id: string;
  width: number;
  height: number;
  pxWidth: number;
  pxHeight: number;
}

type ParaRole =
  | { role: "body" }
  | { role: "heading"; level: number }
  | { role: "list"; ordered: boolean; level: number; numberingId: number }
  | { role: "quote"; depth: number }
  | { role: "code" }
  | { role: "rule" }
  | { role: "cell"; header: boolean };

interface WPara {
  type: "p";
  role: ParaRole;
  align?: "LEFT" | "CENTER" | "RIGHT" | "JUSTIFY";
  runs: WRun[];
}

interface WCell {
  row: number;
  col: number;
  rowSpan: number;
  colSpan: number;
  header: boolean;
  paras: WPara[];
}

interface WTable {
  type: "table";
  rowCount: number;
  colCount: number;
  cells: WCell[];
}

type WBlock = WPara | WTable;

interface BuildState {
  images: { id: string; path: string; bytes: Uint8Array; mediaType: string }[];
  numberings: number;
  textWidth: number;
}

const HEADING_SIZE = [0, 1600, 1400, 1200, 1100, 1100, 1000];

function headingRunFmt(level: number): RunFmt {
  return { bold: true, size: HEADING_SIZE[level] || 1000, gothic: true };
}

function imageRef(img: HTMLImageElement, state: BuildState): ImageRef | null {
  const src = img.getAttribute("src") || "";
  const m = /^data:image\/(png|jpe?g|gif|bmp);base64,(.+)$/i.exec(src);
  if (!m) return null;
  const ext = m[1].toLowerCase() === "jpeg" ? "jpg" : m[1].toLowerCase();
  const bytes = base64ToBytes(m[2]);
  const natural = imageSize(bytes) ?? { width: 480, height: 320 };
  const attrW = Number(img.getAttribute("width")) || natural.width;
  const attrH = Number(img.getAttribute("height")) || (natural.height * attrW) / Math.max(1, natural.width);
  // 96dpi에서 1px = 75 HWPUNIT. 본문 폭을 넘지 않게 줄인다.
  let width = Math.round(attrW * 75);
  let height = Math.round(attrH * 75);
  if (width > state.textWidth) {
    height = Math.round((height * state.textWidth) / width);
    width = state.textWidth;
  }
  const id = `image${state.images.length + 1}`;
  state.images.push({
    id,
    path: `BinData/${id}.${ext}`,
    bytes,
    mediaType: `image/${ext === "jpg" ? "jpeg" : ext}`,
  });
  return { id, width, height, pxWidth: natural.width, pxHeight: natural.height };
}

function inlineRuns(node: Node, fmt: RunFmt, state: BuildState, pre = false): WRun[] {
  const out: WRun[] = [];
  node.childNodes.forEach((child) => {
    if (child.nodeType === 3) {
      let text = child.textContent || "";
      if (!pre) text = text.replace(/\s+/g, " ");
      if (text) out.push({ kind: "text", text, fmt });
      return;
    }
    if (child.nodeType !== 1) return;
    const el = child as HTMLElement;
    switch (el.tagName) {
      case "STRONG":
      case "B":
        out.push(...inlineRuns(el, { ...fmt, bold: true }, state, pre));
        break;
      case "EM":
      case "I":
        out.push(...inlineRuns(el, { ...fmt, italic: true }, state, pre));
        break;
      case "U":
        out.push(...inlineRuns(el, { ...fmt, underline: true }, state, pre));
        break;
      case "S":
      case "DEL":
      case "STRIKE":
        out.push(...inlineRuns(el, { ...fmt, strike: true }, state, pre));
        break;
      case "SUP":
        out.push(...inlineRuns(el, { ...fmt, sup: true }, state, pre));
        break;
      case "SUB":
        out.push(...inlineRuns(el, { ...fmt, sub: true }, state, pre));
        break;
      case "CODE":
        out.push(...inlineRuns(el, { ...fmt, code: true }, state, pre));
        break;
      case "BR":
        out.push({ kind: "br" });
        break;
      case "IMG": {
        const ref = imageRef(el as HTMLImageElement, state);
        if (ref) out.push({ kind: "img", ref });
        break;
      }
      case "A": {
        const href = el.getAttribute("href") || "";
        out.push(...inlineRuns(el, { ...fmt, link: true }, state, pre));
        // 한글 하이퍼링크 필드 대신 주소를 글로 남긴다.
        const text = (el.textContent || "").trim();
        if (/^https?:/i.test(href) && text !== href) out.push({ kind: "text", text: ` (${href})`, fmt });
        break;
      }
      default:
        out.push(...inlineRuns(el, fmt, state, pre));
    }
  });
  return out;
}

function alignOf(el: Element): WPara["align"] {
  const a = ((el as HTMLElement).style?.textAlign || el.getAttribute("align") || "").toLowerCase();
  if (a === "center") return "CENTER";
  if (a === "right") return "RIGHT";
  if (a === "left") return "LEFT";
  return undefined;
}

/** 문단 앞뒤의 공백을 뺀다 (HTML 들여쓰기·줄바꿈에서 생긴 것). */
function trimRuns(runs: WRun[]): WRun[] {
  // 편집기는 빈 문단·빈 칸에 줄바꿈(<br>)을 넣으므로, 문단 앞뒤의 줄바꿈도 공백으로 본다.
  const blank = (r: WRun) => r.kind === "br" || (r.kind === "text" && r.text.trim() === "");
  const out = runs.slice();
  const first = out.findIndex((r) => !blank(r));
  if (first < 0) return [];
  out.splice(0, first);
  const head = out[0];
  if (head.kind === "text") out[0] = { ...head, text: head.text.replace(/^\s+/, "") };
  let last = out.length - 1;
  while (last >= 0 && blank(out[last])) last--;
  out.splice(last + 1);
  const tail = out[out.length - 1];
  if (tail && tail.kind === "text") out[out.length - 1] = { ...tail, text: tail.text.replace(/\s+$/, "") };
  return out;
}

function hasRuns(runs: WRun[]): boolean {
  return runs.some((r) => r.kind !== "text" || r.text.trim() !== "");
}

function listBlocks(list: Element, level: number, state: BuildState, numberingId: number | null): WBlock[] {
  const ordered = list.tagName === "OL";
  // 번호 목록은 목록마다 번호 정의를 따로 두어 1부터 다시 센다.
  const id = ordered ? (numberingId ?? ++state.numberings) : 0;
  const out: WBlock[] = [];
  Array.from(list.children)
    .filter((li) => li.tagName === "LI")
    .forEach((li) => {
      const runs: WRun[] = [];
      const nested: Element[] = [];
      li.childNodes.forEach((n) => {
        if (n.nodeType === 1 && ["UL", "OL"].includes((n as Element).tagName)) {
          nested.push(n as Element);
        } else if (n.nodeType === 1 && (n as Element).tagName === "P") {
          if (hasRuns(runs)) runs.push({ kind: "br" });
          runs.push(...inlineRuns(n, {}, state));
        } else {
          const wrapper = li.ownerDocument.createElement("span");
          wrapper.appendChild(n.cloneNode(true));
          runs.push(...inlineRuns(wrapper, {}, state));
        }
      });
      out.push({ type: "p", role: { role: "list", ordered, level, numberingId: id }, runs });
      nested.forEach((n) =>
        out.push(...listBlocks(n, Math.min(level + 1, 6), state, n.tagName === "OL" && ordered ? id : null))
      );
    });
  return out;
}

function tableBlock(table: Element, state: BuildState): WTable {
  const rows = Array.from(table.querySelectorAll("tr")).filter((tr) => tr.closest("table") === table);
  const occupied: boolean[][] = [];
  const cells: WCell[] = [];
  let colCount = 0;
  rows.forEach((tr, r) => {
    occupied[r] ??= [];
    let c = 0;
    Array.from(tr.children)
      .filter((cell) => cell.tagName === "TD" || cell.tagName === "TH")
      .forEach((cell) => {
        while (occupied[r][c]) c++;
        const colSpan = Math.max(1, Number(cell.getAttribute("colspan")) || 1);
        const rowSpan = Math.max(1, Math.min(rows.length - r, Number(cell.getAttribute("rowspan")) || 1));
        for (let dr = 0; dr < rowSpan; dr++) {
          occupied[r + dr] ??= [];
          for (let dc = 0; dc < colSpan; dc++) occupied[r + dr][c + dc] = true;
        }
        const header = cell.tagName === "TH";
        const blocks = htmlBlocks(cell, state, { quote: 0 });
        let paras = blocks.filter((b): b is WPara => b.type === "p");
        if (paras.length === 0) paras = [{ type: "p", role: { role: "body" }, runs: inlineRuns(cell, {}, state) }];
        paras = paras.map((p) => ({
          ...p,
          role: p.role.role === "body" ? { role: "cell", header } : p.role,
          runs: header ? p.runs.map((x) => (x.kind === "text" ? { ...x, fmt: { ...x.fmt, bold: true } } : x)) : p.runs,
        }));
        cells.push({ row: r, col: c, rowSpan, colSpan, header, paras });
        c += colSpan;
        colCount = Math.max(colCount, c);
      });
  });
  return { type: "table", rowCount: rows.length, colCount: Math.max(1, colCount), cells };
}

function htmlBlocks(root: Node, state: BuildState, ctx: { quote: number }): WBlock[] {
  const out: WBlock[] = [];
  let pending: WRun[] = [];
  const bodyRole = (): ParaRole => (ctx.quote > 0 ? { role: "quote", depth: ctx.quote } : { role: "body" });
  const flush = () => {
    if (hasRuns(pending)) out.push({ type: "p", role: bodyRole(), runs: pending });
    pending = [];
  };

  root.childNodes.forEach((child) => {
    if (child.nodeType === 3) {
      const text = (child.textContent || "").replace(/\s+/g, " ");
      if (text.trim()) pending.push({ kind: "text", text, fmt: {} });
      return;
    }
    if (child.nodeType !== 1) return;
    const el = child as HTMLElement;
    const tag = el.tagName;
    const heading = /^H([1-6])$/.exec(tag);
    if (heading) {
      flush();
      const level = Number(heading[1]);
      out.push({ type: "p", role: { role: "heading", level }, align: alignOf(el), runs: inlineRuns(el, headingRunFmt(level), state) });
    } else if (tag === "P") {
      flush();
      out.push({ type: "p", role: bodyRole(), align: alignOf(el), runs: inlineRuns(el, {}, state) });
    } else if (tag === "UL" || tag === "OL") {
      flush();
      out.push(...listBlocks(el, 0, state, null));
    } else if (tag === "BLOCKQUOTE") {
      flush();
      out.push(...htmlBlocks(el, state, { quote: ctx.quote + 1 }));
    } else if (tag === "PRE") {
      flush();
      const lines = (el.textContent || "").replace(/\n$/, "").split("\n");
      for (const line of lines) {
        out.push({ type: "p", role: { role: "code" }, runs: [{ kind: "text", text: line, fmt: { code: true } }] });
      }
    } else if (tag === "TABLE") {
      flush();
      out.push(tableBlock(el, state));
    } else if (tag === "HR") {
      flush();
      out.push({ type: "p", role: { role: "rule" }, runs: [] });
    } else if (tag === "IMG") {
      flush();
      const ref = imageRef(el as HTMLImageElement, state);
      if (ref) out.push({ type: "p", role: bodyRole(), align: "CENTER", runs: [{ kind: "img", ref }] });
    } else if (["DIV", "SECTION", "ARTICLE", "MAIN", "BODY", "HEADER", "FOOTER", "TBODY", "THEAD"].includes(tag)) {
      flush();
      out.push(...htmlBlocks(el, state, ctx));
    } else {
      pending.push(...inlineRuns({ childNodes: [el] } as unknown as Node, {}, state));
    }
  });
  flush();
  return out;
}

// ---- 모양 목록 (header.xml의 charPr·paraPr) ---------------------------------

/** 글꼴 번호: 0 함초롬돋움, 1 함초롬바탕, 2 Consolas(코드의 영문) */
const FONTS = ["함초롬돋움", "함초롬바탕", "Consolas"];

class Registry<K> {
  private ids = new Map<string, number>();
  readonly items: K[] = [];
  id(item: K): number {
    const key = JSON.stringify(item);
    let id = this.ids.get(key);
    if (id === undefined) {
      id = this.items.length;
      this.ids.set(key, id);
      this.items.push(item);
    }
    return id;
  }
}

interface CharShape {
  size: number;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
  sup: boolean;
  sub: boolean;
  color: string;
  /** 한글 글꼴 / 영문 글꼴 번호 */
  hangulFont: number;
  latinFont: number;
  shade: boolean;
}

interface ParaShape {
  align: string;
  left: number;
  indent: number;
  prev: number;
  next: number;
  lineSpacing: number;
  heading: { type: "NONE" | "BULLET" | "NUMBER"; idRef: number; level: number };
  keepWithNext: boolean;
  /** 문단 테두리·배경 (borderFill 번호, 2는 없음) */
  borderFill: number;
}

/** borderFill 번호: 1 없음(쪽 테두리), 2 없음(글자·문단 기본), 3 표 칸, 4 표 머리글 칸, 5 코드 배경, 6 가로줄, 7 인용 왼쪽 줄 */
const BF_NONE = 2;
const BF_CELL = 3;
const BF_HEADER_CELL = 4;
const BF_CODE = 5;
const BF_RULE = 6;
const BF_QUOTE = 7;

function charShapeOf(fmt: RunFmt): CharShape {
  const code = !!fmt.code;
  return {
    size: fmt.size ?? (code ? 950 : 1000),
    bold: !!fmt.bold,
    italic: !!fmt.italic,
    underline: !!fmt.underline || !!fmt.link,
    strike: !!fmt.strike,
    sup: !!fmt.sup,
    sub: !!fmt.sub,
    color: fmt.link ? "#0563C1" : code ? "#C7254E" : "#000000",
    hangulFont: fmt.gothic || code ? 0 : 1,
    latinFont: code ? 2 : fmt.gothic ? 0 : 1,
    shade: code,
  };
}

const BODY_PARA: ParaShape = {
  align: "JUSTIFY",
  left: 0,
  indent: 0,
  prev: 0,
  next: 400,
  lineSpacing: 160,
  heading: { type: "NONE", idRef: 0, level: 0 },
  keepWithNext: false,
  borderFill: BF_NONE,
};

function paraShapeOf(p: WPara): ParaShape {
  const role = p.role;
  let shape: ParaShape;
  switch (role.role) {
    case "heading":
      shape = {
        ...BODY_PARA,
        align: "LEFT",
        prev: role.level <= 2 ? 1200 : 800,
        next: 400,
        keepWithNext: true,
      };
      break;
    case "list":
      shape = {
        ...BODY_PARA,
        align: "LEFT",
        left: 1000 + role.level * 1500,
        next: 200,
        heading: { type: role.ordered ? "NUMBER" : "BULLET", idRef: role.ordered ? role.numberingId : 1, level: role.level },
      };
      break;
    case "quote":
      shape = { ...BODY_PARA, left: 1500 * role.depth, borderFill: BF_QUOTE };
      break;
    case "code":
      shape = { ...BODY_PARA, align: "LEFT", left: 400, next: 0, lineSpacing: 130, borderFill: BF_CODE };
      break;
    case "rule":
      shape = { ...BODY_PARA, next: 400, lineSpacing: 100, borderFill: BF_RULE };
      break;
    case "cell":
      shape = { ...BODY_PARA, align: role.header ? "CENTER" : "LEFT", next: 0, lineSpacing: 130 };
      break;
    default:
      shape = BODY_PARA;
  }
  return p.align ? { ...shape, align: p.align } : shape;
}

function fontfacesXml(): string {
  const langs = ["HANGUL", "LATIN", "HANJA", "JAPANESE", "OTHER", "SYMBOL", "USER"];
  const fonts = FONTS.map(
    (face, id) =>
      `<hh:font id="${id}" face="${esc(face)}" type="TTF" isEmbedded="0"><hh:typeInfo familyType="${
        id === 1 ? "FCAT_OLDSTYLE" : "FCAT_GOTHIC"
      }" weight="6" proportion="4" contrast="0" strokeVariation="1" armStyle="1" letterform="1" midline="1" xHeight="1"/></hh:font>`
  ).join("");
  return `<hh:fontfaces itemCnt="${langs.length}">${langs
    .map((l) => `<hh:fontface lang="${l}" fontCnt="${FONTS.length}">${fonts}</hh:fontface>`)
    .join("")}</hh:fontfaces>`;
}

function borderXml(side: string, type: string, width = "0.12 mm"): string {
  return `<hh:${side} type="${type}" width="${width}" color="#000000"/>`;
}

function borderFillXml(
  id: number,
  sides: { left?: string; right?: string; top?: string; bottom?: string },
  fill?: string,
  width?: string
): string {
  return (
    `<hh:borderFill id="${id}" threeD="0" shadow="0" centerLine="NONE" breakCellSeparateLine="0">` +
    `<hh:slash type="NONE" Crooked="0" isCounter="0"/><hh:backSlash type="NONE" Crooked="0" isCounter="0"/>` +
    borderXml("leftBorder", sides.left ?? "NONE", width) +
    borderXml("rightBorder", sides.right ?? "NONE", width) +
    borderXml("topBorder", sides.top ?? "NONE", width) +
    borderXml("bottomBorder", sides.bottom ?? "NONE", width) +
    `<hh:diagonal type="SOLID" width="0.1 mm" color="#000000"/>` +
    (fill ? `<hc:fillBrush><hc:winBrush faceColor="${fill}" hatchColor="#999999" alpha="0"/></hc:fillBrush>` : "") +
    `</hh:borderFill>`
  );
}

function borderFillsXml(): string {
  const all = { left: "SOLID", right: "SOLID", top: "SOLID", bottom: "SOLID" };
  const items = [
    borderFillXml(1, {}),
    borderFillXml(2, {}, "none"),
    borderFillXml(3, all),
    borderFillXml(4, all, "#F2F2F2"),
    borderFillXml(5, {}, "#F5F5F4"),
    borderFillXml(6, { bottom: "SOLID" }, undefined, "0.4 mm"),
    borderFillXml(7, { left: "SOLID" }, undefined, "0.7 mm"),
  ];
  return `<hh:borderFills itemCnt="${items.length}">${items.join("")}</hh:borderFills>`;
}

function langAttrs(value: number | string): string {
  return ["hangul", "latin", "hanja", "japanese", "other", "symbol", "user"].map((l) => `${l}="${value}"`).join(" ");
}

function charPrXml(id: number, c: CharShape): string {
  const fontRef = `hangul="${c.hangulFont}" latin="${c.latinFont}" hanja="${c.hangulFont}" japanese="${c.hangulFont}" other="${c.latinFont}" symbol="${c.latinFont}" user="${c.latinFont}"`;
  return (
    `<hh:charPr id="${id}" height="${c.size}" textColor="${c.color}" shadeColor="${c.shade ? "#F2F2F2" : "none"}" useFontSpace="0" useKerning="0" symMark="NONE" borderFillIDRef="${BF_NONE}">` +
    `<hh:fontRef ${fontRef}/><hh:ratio ${langAttrs(100)}/><hh:spacing ${langAttrs(0)}/><hh:relSz ${langAttrs(100)}/><hh:offset ${langAttrs(0)}/>` +
    (c.italic ? "<hh:italic/>" : "") +
    (c.bold ? "<hh:bold/>" : "") +
    `<hh:underline type="${c.underline ? "BOTTOM" : "NONE"}" shape="SOLID" color="${c.underline ? c.color : "#000000"}"/>` +
    `<hh:strikeout shape="${c.strike ? "SOLID" : "NONE"}" color="#000000"/>` +
    `<hh:outline type="NONE"/><hh:shadow type="NONE" color="#B2B2B2" offsetX="10" offsetY="10"/>` +
    (c.sup ? "<hh:supscript/>" : "") +
    (c.sub ? "<hh:subscript/>" : "") +
    `</hh:charPr>`
  );
}

function marginXml(p: ParaShape, scale: number): string {
  const v = (n: number) => `value="${n * scale}" unit="HWPUNIT"`;
  return (
    `<hh:margin><hc:intent ${v(p.indent)}/><hc:left ${v(p.left)}/><hc:right ${v(0)}/><hc:prev ${v(p.prev)}/><hc:next ${v(p.next)}/></hh:margin>` +
    `<hh:lineSpacing type="PERCENT" value="${p.lineSpacing}" unit="HWPUNIT"/>`
  );
}

function paraPrXml(id: number, p: ParaShape): string {
  return (
    `<hh:paraPr id="${id}" tabPrIDRef="0" condense="0" fontLineHeight="0" snapToGrid="1" suppressLineNumbers="0" checked="0">` +
    `<hh:align horizontal="${p.align}" vertical="BASELINE"/>` +
    `<hh:heading type="${p.heading.type}" idRef="${p.heading.idRef}" level="${p.heading.level}"/>` +
    `<hh:breakSetting breakLatinWord="KEEP_WORD" breakNonLatinWord="KEEP_WORD" widowOrphan="0" keepWithNext="${
      p.keepWithNext ? 1 : 0
    }" keepLines="0" pageBreakBefore="0" lineWrap="BREAK"/>` +
    `<hh:autoSpacing eAsianEng="0" eAsianNum="0"/>` +
    // 새 한글은 case(HWPUNIT 그대로)를, 옛 한글은 default(두 배 값)를 읽는다.
    `<hp:switch><hp:case hp:required-namespace="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar">${marginXml(p, 1)}</hp:case>` +
    `<hp:default>${marginXml(p, 2)}</hp:default></hp:switch>` +
    `<hh:border borderFillIDRef="${p.borderFill}" offsetLeft="${p.borderFill === BF_QUOTE ? 400 : 0}" offsetRight="0" offsetTop="0" offsetBottom="0" connect="1" ignoreMargin="0"/>` +
    `</hh:paraPr>`
  );
}

const NUM_FORMATS = ["DIGIT", "HANGUL_SYLLABLE", "CIRCLED_DIGIT", "LATIN_SMALL", "ROMAN_SMALL", "DIGIT", "HANGUL_SYLLABLE"];

function paraHeadXml(level: number, format: string, text: string): string {
  return `<hh:paraHead start="1" level="${level}" align="LEFT" useInstWidth="1" autoIndent="1" widthAdjust="0" textOffsetType="PERCENT" textOffset="50" numFormat="${format}" charPrIDRef="4294967295" checkable="0">${text}</hh:paraHead>`;
}

function numberingsXml(count: number): string {
  // 번호 정의 1은 늘 둔다 (목록이 없어도 한글이 기대하는 구조).
  const n = Math.max(1, count);
  const items: string[] = [];
  for (let id = 1; id <= n; id++) {
    const heads = Array.from({ length: 10 }, (_, i) => {
      const level = i + 1;
      const fmt = NUM_FORMATS[i] ?? "DIGIT";
      return paraHeadXml(level, fmt, fmt === "CIRCLED_DIGIT" ? `^${level}` : `^${level}.`);
    }).join("");
    items.push(`<hh:numbering id="${id}" start="0">${heads}</hh:numbering>`);
  }
  return `<hh:numberings itemCnt="${n}">${items.join("")}</hh:numberings>`;
}

function bulletsXml(): string {
  return (
    `<hh:bullets itemCnt="1"><hh:bullet id="1" char="•" useImage="0">` +
    `<hh:paraHead level="0" align="LEFT" useInstWidth="0" autoIndent="1" widthAdjust="0" textOffsetType="PERCENT" textOffset="50" numFormat="DIGIT" charPrIDRef="4294967295" checkable="0"/>` +
    `</hh:bullet></hh:bullets>`
  );
}

interface StyleDef {
  name: string;
  engName: string;
  para: number;
  char: number;
}

function stylesXml(styles: StyleDef[]): string {
  return `<hh:styles itemCnt="${styles.length}">${styles
    .map(
      (s, id) =>
        `<hh:style id="${id}" type="PARA" name="${esc(s.name)}" engName="${esc(s.engName)}" paraPrIDRef="${s.para}" charPrIDRef="${s.char}" nextStyleIDRef="${
          id >= 2 ? 1 : id
        }" langID="1042" lockForm="0"/>`
    )
    .join("")}</hh:styles>`;
}

const NS_DECL =
  'xmlns:ha="http://www.hancom.co.kr/hwpml/2011/app" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" ' +
  'xmlns:hp10="http://www.hancom.co.kr/hwpml/2016/paragraph" xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" ' +
  'xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core" xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" ' +
  'xmlns:hhs="http://www.hancom.co.kr/hwpml/2011/history" xmlns:hm="http://www.hancom.co.kr/hwpml/2011/master-page" ' +
  'xmlns:hpf="http://www.hancom.co.kr/schema/2011/hpf" xmlns:dc="http://purl.org/dc/elements/1.1/" ' +
  'xmlns:opf="http://www.idpf.org/2007/opf/" xmlns:ooxmlchart="http://www.hancom.co.kr/hwpml/2016/ooxmlchart" ' +
  'xmlns:hwpunitchar="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar" xmlns:epub="http://www.idpf.org/2007/ops" ' +
  'xmlns:config="urn:oasis:names:tc:opendocument:xmlns:config:1.0"';

const XML_HEAD = '<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>';

function headerXml(chars: CharShape[], paras: ParaShape[], styles: StyleDef[], numberings: number): string {
  return (
    `${XML_HEAD}<hh:head ${NS_DECL} version="1.4" secCnt="1">` +
    `<hh:beginNum page="1" footnote="1" endnote="1" pic="1" tbl="1" equation="1"/>` +
    `<hh:refList>` +
    fontfacesXml() +
    borderFillsXml() +
    `<hh:charProperties itemCnt="${chars.length}">${chars.map((c, i) => charPrXml(i, c)).join("")}</hh:charProperties>` +
    `<hh:tabProperties itemCnt="1"><hh:tabPr id="0" autoTabLeft="0" autoTabRight="0"/></hh:tabProperties>` +
    numberingsXml(numberings) +
    bulletsXml() +
    `<hh:paraProperties itemCnt="${paras.length}">${paras.map((p, i) => paraPrXml(i, p)).join("")}</hh:paraProperties>` +
    stylesXml(styles) +
    `</hh:refList>` +
    `<hh:compatibleDocument targetProgram="HWP201X"><hh:layoutCompatibility/></hh:compatibleDocument>` +
    `<hh:docOption><hh:linkinfo path="" pageInherit="0" footnoteInherit="0"/></hh:docOption>` +
    `<hh:trackchageConfig flags="56"/>` +
    `</hh:head>`
  );
}

function secPrXml(page: PageSetup): string {
  const note = (place: string, length: string) =>
    `<hp:autoNumFormat type="DIGIT" userChar="" prefixChar="" suffixChar=")" supscript="0"/>` +
    `<hp:noteLine length="${length}" type="SOLID" width="0.12 mm" color="#000000"/>` +
    `<hp:noteSpacing betweenNotes="283" belowLine="567" aboveLine="850"/>` +
    `<hp:numbering type="CONTINUOUS" newNum="1"/><hp:placement place="${place}" beneathText="0"/>`;
  const pageBorder = (type: string) =>
    `<hp:pageBorderFill type="${type}" borderFillIDRef="1" textBorder="PAPER" headerInside="0" footerInside="0" fillArea="PAPER"><hp:offset left="1417" right="1417" top="1417" bottom="1417"/></hp:pageBorderFill>`;
  return (
    `<hp:secPr id="" textDirection="HORIZONTAL" spaceColumns="1134" tabStop="8000" tabStopVal="4000" tabStopUnit="HWPUNIT" outlineShapeIDRef="1" memoShapeIDRef="0" textVerticalWidthHead="0" masterPageCnt="0">` +
    `<hp:grid lineGrid="0" charGrid="0" wonggojiFormat="0"/>` +
    `<hp:startNum pageStartsOn="BOTH" page="0" pic="0" tbl="0" equation="0"/>` +
    `<hp:visibility hideFirstHeader="0" hideFirstFooter="0" hideFirstMasterPage="0" border="SHOW_ALL" fill="SHOW_ALL" hideFirstPageNum="0" hideFirstEmptyLine="0" showLineNumber="0"/>` +
    `<hp:lineNumberShape restartType="0" countBy="0" distance="0" startNumber="0"/>` +
    `<hp:pagePr landscape="${page.landscape}" width="${page.width}" height="${page.height}" gutterType="LEFT_ONLY">` +
    `<hp:margin header="${page.header}" footer="${page.footer}" gutter="${page.gutter}" left="${page.left}" right="${page.right}" top="${page.top}" bottom="${page.bottom}"/></hp:pagePr>` +
    `<hp:footNotePr>${note("EACH_COLUMN", "-1")}</hp:footNotePr>` +
    `<hp:endNotePr>${note("END_OF_DOCUMENT", "14692344")}</hp:endNotePr>` +
    pageBorder("BOTH") +
    pageBorder("EVEN") +
    pageBorder("ODD") +
    `</hp:secPr>` +
    `<hp:ctrl><hp:colPr id="" type="NEWSPAPER" layout="LEFT" colCount="1" sameSz="1" sameGap="0"/></hp:ctrl>`
  );
}

interface SerializeState {
  chars: Registry<CharShape>;
  paras: Registry<ParaShape>;
  styleOf: (role: ParaRole) => number;
  nextId: number;
  textWidth: number;
}

function textRunXml(text: string, charId: number): string {
  // 탭은 hp:tab 요소로 쓴다.
  const parts = text.split("\t").map((s) => esc(s));
  return `<hp:run charPrIDRef="${charId}"><hp:t>${parts.join('<hp:tab width="4000" leader="NONE" type="LEFT"/>')}</hp:t></hp:run>`;
}

function picXml(ref: ImageRef, s: SerializeState): string {
  const id = s.nextId++;
  const { width: w, height: h } = ref;
  const pxW = ref.pxWidth * 75;
  const pxH = ref.pxHeight * 75;
  return (
    `<hp:pic id="${id}" zOrder="${id}" numberingType="PICTURE" textWrap="TOP_AND_BOTTOM" textFlow="BOTH_SIDES" lock="0" dropcapstyle="None" href="" groupLevel="0" instid="${id}" reverse="0">` +
    `<hp:offset x="0" y="0"/><hp:orgSz width="${pxW}" height="${pxH}"/><hp:curSz width="${w}" height="${h}"/>` +
    `<hp:flip horizontal="0" vertical="0"/><hp:rotationInfo angle="0" centerX="${Math.round(w / 2)}" centerY="${Math.round(h / 2)}" rotateimage="1"/>` +
    `<hp:renderingInfo><hc:transMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/>` +
    `<hc:scaMatrix e1="${(w / pxW).toFixed(6)}" e2="0" e3="0" e4="0" e5="${(h / pxH).toFixed(6)}" e6="0"/>` +
    `<hc:rotMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/></hp:renderingInfo>` +
    `<hc:img binaryItemIDRef="${ref.id}" bright="0" contrast="0" effect="REAL_PIC" alpha="0"/>` +
    `<hp:imgRect><hc:pt0 x="0" y="0"/><hc:pt1 x="${pxW}" y="0"/><hc:pt2 x="${pxW}" y="${pxH}"/><hc:pt3 x="0" y="${pxH}"/></hp:imgRect>` +
    `<hp:imgClip left="0" right="${pxW}" top="0" bottom="${pxH}"/>` +
    `<hp:inMargin left="0" right="0" top="0" bottom="0"/><hp:imgDim dimwidth="${pxW}" dimheight="${pxH}"/><hp:effects/>` +
    `<hp:sz width="${w}" widthRelTo="ABSOLUTE" height="${h}" heightRelTo="ABSOLUTE" protect="0"/>` +
    `<hp:pos treatAsChar="1" affectLSpacing="0" flowWithText="1" allowOverlap="0" holdAnchorAndSO="0" vertRelTo="PARA" horzRelTo="COLUMN" vertAlign="TOP" horzAlign="LEFT" vertOffset="0" horzOffset="0"/>` +
    `<hp:outMargin left="0" right="0" top="0" bottom="0"/>` +
    `</hp:pic>`
  );
}

function paragraphXml(p: WPara, s: SerializeState, prefix = ""): string {
  const paraId = s.paras.id(paraShapeOf(p));
  const styleId = s.styleOf(p.role);
  const defaultChar = s.chars.id(charShapeOf(p.role.role === "heading" ? headingRunFmt(p.role.level) : p.role.role === "code" ? { code: true } : {}));
  let runs = "";
  let lineBreakPending = false;
  const source = p.role.role === "code" ? p.runs : trimRuns(p.runs);
  for (const r of source) {
    if (r.kind === "br") {
      runs += `<hp:run charPrIDRef="${defaultChar}"><hp:t><hp:lineBreak/></hp:t></hp:run>`;
      lineBreakPending = true;
      continue;
    }
    lineBreakPending = false;
    if (r.kind === "img") {
      runs += `<hp:run charPrIDRef="${defaultChar}">${picXml(r.ref, s)}<hp:t/></hp:run>`;
      continue;
    }
    if (!r.text) continue;
    runs += textRunXml(r.text, s.chars.id(charShapeOf(r.fmt)));
  }
  if (!runs || lineBreakPending) runs += `<hp:run charPrIDRef="${defaultChar}"><hp:t/></hp:run>`;
  return `<hp:p id="${s.nextId++}" paraPrIDRef="${paraId}" styleIDRef="${styleId}" pageBreak="0" columnBreak="0" merged="0">${prefix}${runs}</hp:p>`;
}

const CELL_MARGIN = { left: 510, right: 510, top: 141, bottom: 141 };

function tableXml(t: WTable, s: SerializeState): string {
  const tableWidth = s.textWidth;
  const colWidth = Math.floor(tableWidth / t.colCount);
  const rowHeight = 1800;
  const byRow = new Map<number, WCell[]>();
  for (const c of t.cells) {
    if (!byRow.has(c.row)) byRow.set(c.row, []);
    byRow.get(c.row)!.push(c);
  }
  let rows = "";
  for (let r = 0; r < t.rowCount; r++) {
    const cells = (byRow.get(r) ?? [])
      .map((c) => {
        const paras = c.paras.map((p) => paragraphXml(p, s)).join("");
        return (
          `<hp:tc name="" header="${c.header ? 1 : 0}" hasMargin="0" protect="0" editable="0" dirty="0" borderFillIDRef="${c.header ? BF_HEADER_CELL : BF_CELL}">` +
          `<hp:subList id="" textDirection="HORIZONTAL" lineWrap="BREAK" vertAlign="CENTER" linkListIDRef="0" linkListNextIDRef="0" textWidth="0" textHeight="0" hasTextRef="0" hasNumRef="0">${paras}</hp:subList>` +
          `<hp:cellAddr colAddr="${c.col}" rowAddr="${c.row}"/><hp:cellSpan colSpan="${c.colSpan}" rowSpan="${c.rowSpan}"/>` +
          `<hp:cellSz width="${colWidth * c.colSpan}" height="${rowHeight * c.rowSpan}"/>` +
          `<hp:cellMargin left="${CELL_MARGIN.left}" right="${CELL_MARGIN.right}" top="${CELL_MARGIN.top}" bottom="${CELL_MARGIN.bottom}"/>` +
          `</hp:tc>`
        );
      })
      .join("");
    rows += `<hp:tr>${cells}</hp:tr>`;
  }
  const id = s.nextId++;
  const tbl =
    `<hp:tbl id="${id}" zOrder="0" numberingType="TABLE" textWrap="TOP_AND_BOTTOM" textFlow="BOTH_SIDES" lock="0" dropcapstyle="None" pageBreak="CELL" repeatHeader="1" rowCnt="${t.rowCount}" colCnt="${t.colCount}" cellSpacing="0" borderFillIDRef="${BF_CELL}" noAdjust="0">` +
    `<hp:sz width="${colWidth * t.colCount}" widthRelTo="ABSOLUTE" height="${rowHeight * t.rowCount}" heightRelTo="ABSOLUTE" protect="0"/>` +
    `<hp:pos treatAsChar="1" affectLSpacing="0" flowWithText="1" allowOverlap="0" holdAnchorAndSO="0" vertRelTo="PARA" horzRelTo="PARA" vertAlign="TOP" horzAlign="LEFT" vertOffset="0" horzOffset="0"/>` +
    `<hp:outMargin left="0" right="0" top="0" bottom="0"/>` +
    `<hp:inMargin left="${CELL_MARGIN.left}" right="${CELL_MARGIN.right}" top="${CELL_MARGIN.top}" bottom="${CELL_MARGIN.bottom}"/>` +
    rows +
    `</hp:tbl>`;
  const paraId = s.paras.id({ ...BODY_PARA, next: 400 });
  return `<hp:p id="${s.nextId++}" paraPrIDRef="${paraId}" styleIDRef="1" pageBreak="0" columnBreak="0" merged="0"><hp:run charPrIDRef="0">${tbl}<hp:t/></hp:run></hp:p>`;
}

function plainText(blocks: WBlock[]): string {
  const lines: string[] = [];
  for (const b of blocks) {
    if (b.type === "p") {
      lines.push(b.runs.map((r) => (r.kind === "text" ? r.text : r.kind === "br" ? "\n" : "")).join(""));
    } else {
      for (const c of b.cells) lines.push(plainText(c.paras));
    }
  }
  return lines.join("\r\n");
}

export interface HwpxWriteOptions {
  /** 이 HWPX의 용지 설정(크기·여백)을 그대로 쓴다 (캔버스에서 기존 문서를 저장할 때) */
  base?: Uint8Array;
}

/** 기존 HWPX의 첫 구역 용지 설정을 읽는다. 없거나 읽을 수 없으면 null. */
export async function readPageSetup(bytes: Uint8Array): Promise<PageSetup | null> {
  try {
    const pkg = await openPackage(bytes);
    const file = pkg.zip.file(pkg.sections[0]);
    if (!file) return null;
    const doc = parseXml(await file.async("string"));
    const pagePr = descendants(doc, "pagePr")[0];
    const margin = kid(pagePr, "margin");
    if (!pagePr || !margin) return null;
    const n = (el: Element, a: string, d: number) => int(el.getAttribute(a), d);
    const page: PageSetup = {
      width: n(pagePr, "width", A4_PAGE.width),
      height: n(pagePr, "height", A4_PAGE.height),
      landscape: pagePr.getAttribute("landscape") === "NARROWLY" ? "NARROWLY" : "WIDELY",
      left: n(margin, "left", A4_PAGE.left),
      right: n(margin, "right", A4_PAGE.right),
      top: n(margin, "top", A4_PAGE.top),
      bottom: n(margin, "bottom", A4_PAGE.bottom),
      header: n(margin, "header", A4_PAGE.header),
      footer: n(margin, "footer", A4_PAGE.footer),
      gutter: n(margin, "gutter", 0),
    };
    // 이상한 값이면 쓰지 않는다.
    if (page.width < 10000 || page.height < 10000 || page.width - page.left - page.right < 5000) return null;
    return page;
  } catch {
    return null;
  }
}

/** HWPX 꾸러미를 쓴다. mimetype은 압축하지 않고 맨 앞에 둔다. */
async function writePackage(files: { path: string; data: string | Uint8Array }[]): Promise<Uint8Array> {
  const zip = new JSZip();
  zip.file("mimetype", MIMETYPE, { compression: "STORE" });
  for (const f of files) zip.file(f.path, f.data);
  return zip.generateAsync({ type: "uint8array", compression: "DEFLATE", mimeType: MIMETYPE });
}

export async function htmlToHwpx(html: string, title = "문서", opts: HwpxWriteOptions = {}): Promise<Uint8Array> {
  const page = (opts.base && (await readPageSetup(opts.base))) || A4_PAGE;
  // 본문 폭: 용지 폭(가로 방향이면 높이) − 왼쪽·오른쪽 여백
  const paperWidth = page.landscape === "NARROWLY" ? page.height : page.width;
  const textWidth = Math.max(5000, paperWidth - page.left - page.right - page.gutter);

  const dom = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
  const state: BuildState = { images: [], numberings: 0, textWidth };
  const blocks = htmlBlocks(dom.body, state, { quote: 0 });

  const chars = new Registry<CharShape>();
  const paras = new Registry<ParaShape>();
  // 0번은 바탕글 모양
  const bodyChar = chars.id(charShapeOf({}));
  const bodyPara = paras.id(BODY_PARA);
  const styles: StyleDef[] = [
    { name: "바탕글", engName: "Normal", para: bodyPara, char: bodyChar },
    { name: "본문", engName: "Body", para: bodyPara, char: bodyChar },
  ];
  for (let level = 1; level <= 6; level++) {
    styles.push({
      name: `제목 ${level}`,
      engName: `Heading ${level}`,
      para: paras.id(paraShapeOf({ type: "p", role: { role: "heading", level }, runs: [] })),
      char: chars.id(charShapeOf(headingRunFmt(level))),
    });
  }
  const s: SerializeState = {
    chars,
    paras,
    styleOf: (role) => (role.role === "heading" ? 1 + role.level : role.role === "body" ? 0 : 1),
    nextId: 1,
    textWidth,
  };

  // 구역 설정(secPr)은 첫 문단의 첫 글자 묶음에 들어간다.
  const secRun = `<hp:run charPrIDRef="${bodyChar}">${secPrXml(page)}</hp:run>`;
  let body = "";
  if (blocks.length === 0 || blocks[0].type !== "p") {
    body += `<hp:p id="0" paraPrIDRef="${bodyPara}" styleIDRef="0" pageBreak="0" columnBreak="0" merged="0">${secRun}<hp:run charPrIDRef="${bodyChar}"><hp:t/></hp:run></hp:p>`;
  }
  blocks.forEach((b, i) => {
    if (b.type === "table") body += tableXml(b, s);
    else body += paragraphXml(b, s, i === 0 ? secRun : "");
  });

  const section = `${XML_HEAD}<hs:sec ${NS_DECL}>${body}</hs:sec>`;
  const header = headerXml(chars.items, paras.items, styles, state.numberings);
  const now = new Date().toISOString().replace(/\.\d+Z$/, "Z");
  const imageItems = state.images
    .map((im) => `<opf:item id="${im.id}" href="${im.path}" media-type="${im.mediaType}" isEmbeded="1"/>`)
    .join("");
  const contentHpf =
    `${XML_HEAD}<opf:package ${NS_DECL} version="" unique-identifier="" id="">` +
    `<opf:metadata><opf:title>${esc(title)}</opf:title><opf:language>ko</opf:language>` +
    `<opf:meta name="creator" content="text">Any LM Desktop</opf:meta>` +
    `<opf:meta name="CreatedDate" content="text">${now}</opf:meta><opf:meta name="ModifiedDate" content="text">${now}</opf:meta></opf:metadata>` +
    `<opf:manifest><opf:item id="header" href="Contents/header.xml" media-type="application/xml"/>` +
    `<opf:item id="section0" href="Contents/section0.xml" media-type="application/xml"/>` +
    `<opf:item id="settings" href="settings.xml" media-type="application/xml"/>${imageItems}</opf:manifest>` +
    `<opf:spine><opf:itemref idref="header" linear="yes"/><opf:itemref idref="section0" linear="yes"/></opf:spine>` +
    `</opf:package>`;

  const container =
    `${XML_HEAD}<ocf:container xmlns:ocf="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:hpf="http://www.hancom.co.kr/schema/2011/hpf">` +
    `<ocf:rootfiles><ocf:rootfile full-path="Contents/content.hpf" media-type="application/hwpml-package+xml"/>` +
    `<ocf:rootfile full-path="Preview/PrvText.txt" media-type="text/plain"/>` +
    `<ocf:rootfile full-path="META-INF/container.rdf" media-type="application/rdf+xml"/></ocf:rootfiles></ocf:container>`;
  const part = (path: string, type: string) =>
    `<rdf:Description rdf:about=""><ns0:hasPart xmlns:ns0="http://www.hancom.co.kr/hwpml/2016/meta/pkg#" rdf:resource="${path}"/></rdf:Description>` +
    `<rdf:Description rdf:about="${path}"><rdf:type rdf:resource="http://www.hancom.co.kr/hwpml/2016/meta/pkg#${type}"/></rdf:Description>`;
  const rdf =
    `${XML_HEAD}<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">` +
    part("Contents/header.xml", "HeaderFile") +
    part("Contents/section0.xml", "SectionFile") +
    `<rdf:Description rdf:about=""><rdf:type rdf:resource="http://www.hancom.co.kr/hwpml/2016/meta/pkg#Document"/></rdf:Description></rdf:RDF>`;
  const version =
    `${XML_HEAD}<hv:HCFVersion xmlns:hv="http://www.hancom.co.kr/hwpml/2011/version" tagetApplication="WORDPROCESSOR" major="5" minor="1" micro="0" buildNumber="1" os="1" xmlVersion="1.4" application="Any LM Desktop" appVersion="1"/>`;
  const settings =
    `${XML_HEAD}<ha:HWPApplicationSetting xmlns:ha="http://www.hancom.co.kr/hwpml/2011/app" xmlns:config="urn:oasis:names:tc:opendocument:xmlns:config:1.0">` +
    `<ha:CaretPosition listIDRef="0" paraIDRef="0" pos="0"/></ha:HWPApplicationSetting>`;

  return writePackage([
    { path: "version.xml", data: version },
    { path: "Contents/header.xml", data: header },
    { path: "Contents/section0.xml", data: section },
    { path: "Preview/PrvText.txt", data: plainText(blocks).slice(0, 1024) },
    { path: "settings.xml", data: settings },
    { path: "META-INF/container.rdf", data: rdf },
    { path: "Contents/content.hpf", data: contentHpf },
    { path: "META-INF/container.xml", data: container },
    {
      path: "META-INF/manifest.xml",
      data: `${XML_HEAD}<odf:manifest xmlns:odf="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"/>`,
    },
    ...state.images.map((im) => ({ path: im.path, data: im.bytes })),
  ]);
}

export async function markdownToHwpx(md: string, title = "문서"): Promise<Uint8Array> {
  return htmlToHwpx(markdownToHtml(md), title);
}

// ---------------------------------------------------------------------------
// 서식을 유지한 문구 바꾸기
// ---------------------------------------------------------------------------

/** 문단 p에 속한 글자 노드들 (안에 든 표·글상자의 문단은 뺀다) */
function paragraphTextNodes(p: Element): Text[] {
  const out: Text[] = [];
  for (const t of descendants(p, "t")) {
    if (t.namespaceURI !== HP_NS) continue;
    // 가장 가까운 hp:p 조상이 p인 것만
    let cur: Element | null = t.parentElement;
    while (cur && !(cur.localName === "p" && cur.namespaceURI === HP_NS)) cur = cur.parentElement;
    if (cur !== p) continue;
    t.childNodes.forEach((n) => {
      if (n.nodeType === 3) out.push(n as Text);
    });
  }
  return out;
}

function replaceInParagraph(p: Element, target: string, replacement: string): boolean {
  const nodes = paragraphTextNodes(p);
  const texts = nodes.map((n) => n.nodeValue ?? "");
  const start = texts.join("").indexOf(target);
  if (start < 0) return false;
  const end = start + target.length;
  let offset = 0;
  let state: "before" | "inside" | "after" = "before";
  nodes.forEach((node, i) => {
    const text = texts[i];
    const s = offset;
    const e = s + text.length;
    offset = e;
    if (state === "before" && start < e) {
      const head = text.slice(0, start - s);
      if (end <= e) {
        node.nodeValue = head + replacement + text.slice(end - s);
        state = "after";
      } else {
        node.nodeValue = head + replacement;
        state = "inside";
      }
    } else if (state === "inside") {
      if (end <= e) {
        node.nodeValue = text.slice(end - s);
        state = "after";
      } else {
        node.nodeValue = "";
      }
    }
  });
  if (state === "before") return false;
  // 줄 배치 정보가 바뀐 글과 맞지 않으므로 지운다 (한글이 열 때 다시 계산한다).
  kids(p, "linesegarray").forEach((l) => l.remove());
  return true;
}

/** 꾸러미의 파일을 같은 순서로 다시 쓴다 (mimetype은 압축 없이 맨 앞). */
async function rewritePackage(zip: JSZip, changed: Map<string, string>): Promise<Uint8Array> {
  const files: { path: string; data: string | Uint8Array }[] = [];
  for (const name of Object.keys(zip.files)) {
    const f = zip.files[name];
    if (f.dir || name === "mimetype") continue;
    files.push({ path: name, data: changed.get(name) ?? (await f.async("uint8array")) });
  }
  return writePackage(files);
}

/**
 * HWPX 본문에서 target을 찾아 replacement로 바꾼다. 글자 모양은 그대로 둔다.
 * 대상은 한 문단(또는 표의 한 칸 안의 한 문단) 안에 있어야 한다.
 */
export async function replaceTextInHwpx(bytes: Uint8Array, target: string, replacement: string): Promise<Uint8Array> {
  if (!target) throw new Error("target이 비어 있습니다.");
  const pkg = await openPackage(bytes);
  for (const path of pkg.sections) {
    const file = pkg.zip.file(path);
    if (!file) continue;
    const doc = parseXml(await file.async("string"));
    for (const p of descendants(doc, "p")) {
      if (p.namespaceURI !== HP_NS) continue;
      if (replaceInParagraph(p, target, replacement)) {
        return rewritePackage(pkg.zip, new Map([[path, serializeXml(doc)]]));
      }
    }
  }
  throw new Error(
    "문서에서 대상 문자열(target)을 한 문단 안에서 찾지 못했습니다. read_file로 현재 내용을 확인하고, 한 문단 안의 정확한 문구를 지정하세요."
  );
}

/** HWPX 파일인지 바이트로 확인한다 (ZIP이고 mimetype이 application/hwp+zip). */
export function looksLikeHwpx(bytes: Uint8Array): boolean {
  if (bytes.length < 60 || bytes[0] !== 0x50 || bytes[1] !== 0x4b) return false;
  const head = new TextDecoder().decode(bytes.subarray(30, 60));
  return head.startsWith("mimetype") && head.includes(MIMETYPE.slice(0, 15));
}
