import JSZip from "jszip";
import { marked, type Token, type Tokens } from "marked";
import PptxGenJS from "pptxgenjs";
import { bytesToBase64 } from "./base64";

export const A_NS = "http://schemas.openxmlformats.org/drawingml/2006/main";
const R_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const XML_NS = "http://www.w3.org/XML/1998/namespace";
const EMU_PER_PT = 12700;

// ---------------------------------------------------------------------------
// 모델
// ---------------------------------------------------------------------------

export interface PptxRun {
  text: string;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  sizePt: number;
  color?: string;
  font?: string;
}

export interface PptxParagraph {
  key: string;
  runs: PptxRun[];
  text: string;
  level: number;
  bullet: string | null;
  align: "left" | "center" | "right" | "justify";
  /** 빈 문단일 때 높이 계산에 쓸 글꼴 크기 */
  sizePt: number;
  /** 슬라이드 번호·날짜처럼 PowerPoint가 채우는 필드만 있는 문단 */
  readOnly: boolean;
}

export interface PptxShape {
  key: string;
  type: "text" | "image" | "table" | "shape";
  x: number;
  y: number;
  w: number;
  h: number;
  rot: number;
  fill?: string;
  line?: string;
  geom?: string;
  anchor: "top" | "middle" | "bottom";
  insets: [number, number, number, number];
  paragraphs: PptxParagraph[];
  imageSrc?: string;
  table?: { rows: string[][]; colWidths: number[] };
  placeholder?: string;
}

export interface PptxSlide {
  index: number;
  path: string;
  shapes: PptxShape[];
  background: string;
  notes: string;
}

export interface PptxDeck {
  width: number;
  height: number;
  slides: PptxSlide[];
}

export interface PptxSession {
  deck: PptxDeck;
  zip: JSZip;
  docs: Map<string, Document>;
  paragraphEls: Map<string, Element>;
}

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

function path(el: Element | null | undefined, ...names: string[]): Element | null {
  let cur: Element | null | undefined = el;
  for (const n of names) {
    cur = kid(cur, n);
    if (!cur) return null;
  }
  return cur ?? null;
}

function descendants(el: Element | Document, name: string): Element[] {
  return Array.from(el.getElementsByTagName("*")).filter((c) => c.localName === name);
}

function num(v: string | null | undefined, fallback = 0): number {
  const n = Number(v);
  return Number.isFinite(n) && v !== null && v !== undefined && v !== "" ? n : fallback;
}

function parseXml(text: string): Document {
  return new DOMParser().parseFromString(text, "application/xml");
}

function serializeXml(doc: Document): string {
  const body = new XMLSerializer().serializeToString(doc);
  return body.startsWith("<?xml")
    ? body
    : `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n${body}`;
}

function dirOf(p: string): string {
  const i = p.lastIndexOf("/");
  return i >= 0 ? p.slice(0, i) : "";
}

export function resolvePartPath(baseDir: string, target: string): string {
  if (target.startsWith("/")) return target.slice(1);
  const parts = (baseDir ? baseDir.split("/") : []).concat(target.split("/"));
  const out: string[] = [];
  for (const p of parts) {
    if (p === "..") out.pop();
    else if (p !== "." && p !== "") out.push(p);
  }
  return out.join("/");
}

function relsPathOf(part: string): string {
  const dir = dirOf(part);
  const file = part.slice(dir.length + (dir ? 1 : 0));
  return `${dir ? dir + "/" : ""}_rels/${file}.rels`;
}

interface Rel {
  id: string;
  type: string;
  target: string;
  external: boolean;
}

async function readXml(zip: JSZip, p: string): Promise<Document | null> {
  const f = zip.file(p);
  return f ? parseXml(await f.async("string")) : null;
}

async function readRels(zip: JSZip, part: string): Promise<Map<string, Rel>> {
  const doc = await readXml(zip, relsPathOf(part));
  const map = new Map<string, Rel>();
  if (!doc) return map;
  for (const r of descendants(doc, "Relationship")) {
    const external = r.getAttribute("TargetMode") === "External";
    const target = r.getAttribute("Target") || "";
    map.set(r.getAttribute("Id") || "", {
      id: r.getAttribute("Id") || "",
      type: r.getAttribute("Type") || "",
      target: external ? target : resolvePartPath(dirOf(part), target),
      external,
    });
  }
  return map;
}

function relByType(rels: Map<string, Rel>, suffix: string): Rel | undefined {
  return Array.from(rels.values()).find((r) => r.type.endsWith(suffix));
}

/** r:id, r:embed 같은 관계 속성을 읽는다. (DOM 구현마다 접두어 속성 처리가 달라 직접 찾는다) */
function rAttr(el: Element, name: string): string | null {
  for (const a of Array.from(el.attributes)) {
    if (a.name === `r:${name}` || (a.localName === name && a.namespaceURI === R_NS)) return a.value;
  }
  return null;
}

function setXmlSpacePreserve(el: Element) {
  const existing = Array.from(el.attributes).find((a) => a.name === "xml:space");
  if (existing) existing.value = "preserve";
  else el.setAttributeNS(XML_NS, "xml:space", "preserve");
}

// ---------------------------------------------------------------------------
// 색상 / 테마
// ---------------------------------------------------------------------------

type Theme = Record<string, string>;

async function readTheme(zip: JSZip, masterPath: string): Promise<Theme> {
  const rels = await readRels(zip, masterPath);
  const rel = relByType(rels, "/theme");
  const doc = rel ? await readXml(zip, rel.target) : null;
  const theme: Theme = {};
  const scheme = doc ? descendants(doc, "clrScheme")[0] : null;
  for (const c of scheme ? Array.from(scheme.children) : []) {
    const srgb = kid(c, "srgbClr")?.getAttribute("val");
    const sys = kid(c, "sysClr")?.getAttribute("lastClr");
    const hex = safeHex(srgb || sys);
    if (hex) theme[c.localName] = hex;
  }
  return theme;
}

/** 파일에서 읽은 색 값은 6자리 16진수만 받는다 (HTML·CSS에 그대로 들어가므로). */
/** 글꼴 이름에는 글자·숫자·공백·하이픈 등만 남긴다 (CSS에 들어가므로). */
export function safeFontName(v: string | null | undefined): string | undefined {
  const t = (v ?? "").replace(/[^\p{L}\p{N} _.+\-]/gu, "").trim();
  return t || undefined;
}

export function safeHex(v: string | null | undefined): string | undefined {
  const t = (v ?? "").trim().toUpperCase();
  return /^[0-9A-F]{6}$/.test(t) ? t : undefined;
}

function applyLum(hex: string, colorEl: Element): string {
  const mod = num(kid(colorEl, "lumMod")?.getAttribute("val"), 100000) / 100000;
  const off = num(kid(colorEl, "lumOff")?.getAttribute("val"), 0) / 100000;
  if (mod === 1 && off === 0) return hex;
  const ch = [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16));
  return ch
    .map((v) => Math.max(0, Math.min(255, Math.round(v * mod + 255 * off))).toString(16).padStart(2, "0"))
    .join("")
    .toUpperCase();
}

const SCHEME_ALIAS: Record<string, string> = { bg1: "lt1", tx1: "dk1", bg2: "lt2", tx2: "dk2" };

function colorFrom(container: Element | null, theme: Theme): string | undefined {
  if (!container) return undefined;
  const srgb = kid(container, "srgbClr");
  if (srgb) return "#" + applyLum(safeHex(srgb.getAttribute("val")) ?? "000000", srgb);
  const scheme = kid(container, "schemeClr");
  if (scheme) {
    const name = scheme.getAttribute("val") || "";
    const base = theme[SCHEME_ALIAS[name] || name];
    return base ? "#" + applyLum(base, scheme) : undefined;
  }
  const sys = kid(container, "sysClr");
  if (sys) return "#" + (safeHex(sys.getAttribute("lastClr")) ?? "000000");
  return undefined;
}

function fillOf(spPr: Element | null, theme: Theme): string | undefined {
  if (!spPr || kid(spPr, "noFill")) return undefined;
  const solid = kid(spPr, "solidFill");
  if (solid) return colorFrom(solid, theme);
  const grad = kid(spPr, "gradFill");
  const firstStop = grad ? descendants(grad, "gs")[0] : null;
  return firstStop ? colorFrom(firstStop, theme) : undefined;
}

// ---------------------------------------------------------------------------
// 자리표시자 상속
// ---------------------------------------------------------------------------

interface Placeholder {
  type: string;
  idx: string | null;
  xfrm: { x: number; y: number; w: number; h: number } | null;
  sizes: (number | undefined)[];
  anchor?: string;
}

const TITLE_TYPES = ["title", "ctrTitle"];

function phInfo(sp: Element): { type: string; idx: string | null } | null {
  const ph = path(sp, "nvSpPr", "nvPr", "ph") || path(sp, "nvPicPr", "nvPr", "ph");
  if (!ph) return null;
  return { type: ph.getAttribute("type") || "body", idx: ph.getAttribute("idx") };
}

function xfrmOf(el: Element | null): { x: number; y: number; w: number; h: number; rot: number } | null {
  if (!el) return null;
  const off = kid(el, "off");
  const ext = kid(el, "ext");
  if (!off || !ext) return null;
  return {
    x: num(off.getAttribute("x")),
    y: num(off.getAttribute("y")),
    w: num(ext.getAttribute("cx")),
    h: num(ext.getAttribute("cy")),
    rot: num(el.getAttribute("rot")) / 60000,
  };
}

function levelSizes(lstStyle: Element | null): (number | undefined)[] {
  return Array.from({ length: 9 }, (_, i) => {
    const sz = path(lstStyle, `lvl${i + 1}pPr`, "defRPr")?.getAttribute("sz");
    return sz ? num(sz) / 100 : undefined;
  });
}

function collectPlaceholders(doc: Document | null): Placeholder[] {
  if (!doc) return [];
  return descendants(doc, "sp")
    .map((sp) => {
      const info = phInfo(sp);
      if (!info) return null;
      const x = xfrmOf(path(sp, "spPr", "xfrm"));
      return {
        ...info,
        xfrm: x ? { x: x.x, y: x.y, w: x.w, h: x.h } : null,
        sizes: levelSizes(path(sp, "txBody", "lstStyle")),
        anchor: path(sp, "txBody", "bodyPr")?.getAttribute("anchor") || undefined,
      } as Placeholder;
    })
    .filter((p): p is Placeholder => p !== null);
}

function matchPlaceholder(list: Placeholder[], info: { type: string; idx: string | null }): Placeholder | undefined {
  const isTitle = TITLE_TYPES.includes(info.type);
  return (
    (info.idx !== null ? list.find((p) => p.idx === info.idx) : undefined) ||
    list.find((p) => p.type === info.type) ||
    (isTitle ? list.find((p) => TITLE_TYPES.includes(p.type)) : undefined) ||
    (!isTitle ? list.find((p) => ["body", "obj"].includes(p.type)) : undefined)
  );
}

interface MasterStyles {
  title: (number | undefined)[];
  body: (number | undefined)[];
  other: (number | undefined)[];
}

function masterStyles(master: Document | null): MasterStyles {
  const tx = master ? descendants(master, "txStyles")[0] : null;
  return {
    title: levelSizes(kid(tx, "titleStyle")),
    body: levelSizes(kid(tx, "bodyStyle")),
    other: levelSizes(kid(tx, "otherStyle")),
  };
}

// ---------------------------------------------------------------------------
// 슬라이드 파싱
// ---------------------------------------------------------------------------

interface SlideContext {
  slideIndex: number;
  theme: Theme;
  layoutPh: Placeholder[];
  masterPh: Placeholder[];
  styles: MasterStyles;
  rels: Map<string, Rel>;
  zip: JSZip;
  paragraphEls: Map<string, Element>;
  counter: { n: number };
  withImages: boolean;
}

interface GroupTransform {
  offX: number;
  offY: number;
  chOffX: number;
  chOffY: number;
  scaleX: number;
  scaleY: number;
}

const IDENTITY: GroupTransform = { offX: 0, offY: 0, chOffX: 0, chOffY: 0, scaleX: 1, scaleY: 1 };

function applyTransform(t: GroupTransform, r: { x: number; y: number; w: number; h: number }) {
  return {
    x: t.offX + (r.x - t.chOffX) * t.scaleX,
    y: t.offY + (r.y - t.chOffY) * t.scaleY,
    w: r.w * t.scaleX,
    h: r.h * t.scaleY,
  };
}

const IMAGE_MIME: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  bmp: "image/bmp",
  svg: "image/svg+xml",
  webp: "image/webp",
};

function paragraphText(p: Element): string {
  let text = "";
  for (const c of Array.from(p.children)) {
    if (c.localName === "r" || c.localName === "fld") text += kid(c, "t")?.textContent ?? "";
    else if (c.localName === "br") text += "\n";
  }
  return text;
}

function parseTextBody(
  txBody: Element | null,
  ctx: SlideContext,
  shapeKey: string,
  ph: { type: string; idx: string | null } | null,
  inherited: Placeholder | undefined,
  masterInherited: Placeholder | undefined
): PptxParagraph[] {
  if (!txBody) return [];
  const isTitle = ph ? TITLE_TYPES.includes(ph.type) : false;
  const isBody = ph ? ["body", "obj"].includes(ph.type) : false;
  const styleSizes = isTitle ? ctx.styles.title : ph ? ctx.styles.body : ctx.styles.other;
  const fontScale = num(path(txBody, "bodyPr", "normAutofit")?.getAttribute("fontScale"), 100000) / 100000;
  const ownSizes = levelSizes(kid(txBody, "lstStyle"));
  const autoNum: number[] = [];

  return kids(txBody, "p").map((p, pIdx) => {
    const pPr = kid(p, "pPr");
    const level = num(pPr?.getAttribute("lvl"));
    const defaultSize =
      ownSizes[level] ??
      inherited?.sizes[level] ??
      masterInherited?.sizes[level] ??
      styleSizes[level] ??
      (isTitle ? 32 : 18);

    const runs: PptxRun[] = [];
    for (const c of Array.from(p.children)) {
      if (c.localName === "br") {
        runs.push({ text: "\n", sizePt: defaultSize * fontScale });
        continue;
      }
      if (c.localName !== "r" && c.localName !== "fld") continue;
      const rPr = kid(c, "rPr");
      const sz = rPr?.getAttribute("sz");
      runs.push({
        text: kid(c, "t")?.textContent ?? "",
        bold: rPr?.getAttribute("b") === "1" || rPr?.getAttribute("b") === "true",
        italic: rPr?.getAttribute("i") === "1" || rPr?.getAttribute("i") === "true",
        underline: !!rPr?.getAttribute("u") && rPr?.getAttribute("u") !== "none",
        sizePt: (sz ? num(sz) / 100 : defaultSize) * fontScale,
        color: colorFrom(kid(rPr, "solidFill"), ctx.theme),
        font: safeFontName(kid(rPr, "latin")?.getAttribute("typeface")),
      });
    }

    let bullet: string | null = null;
    if (pPr && kid(pPr, "buNone")) bullet = null;
    else if (pPr && kid(pPr, "buChar")) bullet = kid(pPr, "buChar")!.getAttribute("char") || "•";
    else if (pPr && kid(pPr, "buAutoNum")) {
      autoNum[level] = (autoNum[level] ?? 0) + 1;
      autoNum.length = level + 1;
      bullet = `${autoNum[level]}.`;
    } else if (isBody && runs.some((r) => r.text.trim())) bullet = "•";

    const algn = pPr?.getAttribute("algn");
    const key = `${shapeKey}:${pIdx}`;
    ctx.paragraphEls.set(key, p);
    const endSz = kid(p, "endParaRPr")?.getAttribute("sz");
    return {
      key,
      runs,
      text: paragraphText(p),
      level,
      bullet,
      align: algn === "ctr" ? "center" : algn === "r" ? "right" : algn === "just" ? "justify" : "left",
      sizePt: (endSz ? num(endSz) / 100 : defaultSize) * fontScale,
      readOnly: kids(p, "fld").length > 0 && kids(p, "r").length === 0,
    } as PptxParagraph;
  });
}

async function parseShapes(container: Element, ctx: SlideContext, t: GroupTransform): Promise<PptxShape[]> {
  const shapes: PptxShape[] = [];
  for (const el of Array.from(container.children)) {
    const name = el.localName;
    if (name === "grpSp") {
      const gx = path(el, "grpSpPr", "xfrm");
      const own = xfrmOf(gx);
      const chOff = kid(gx, "chOff");
      const chExt = kid(gx, "chExt");
      if (own && chOff && chExt) {
        const outer = applyTransform(t, own);
        const chW = num(chExt.getAttribute("cx"), own.w) || 1;
        const chH = num(chExt.getAttribute("cy"), own.h) || 1;
        shapes.push(
          ...(await parseShapes(el, ctx, {
            offX: outer.x,
            offY: outer.y,
            chOffX: num(chOff.getAttribute("x")),
            chOffY: num(chOff.getAttribute("y")),
            scaleX: outer.w / chW,
            scaleY: outer.h / chH,
          }))
        );
      } else {
        shapes.push(...(await parseShapes(el, ctx, t)));
      }
      continue;
    }
    if (!["sp", "pic", "graphicFrame", "cxnSp"].includes(name)) continue;

    const shapeKey = `${ctx.slideIndex}:${ctx.counter.n++}`;
    const ph = name === "sp" || name === "pic" ? phInfo(el) : null;
    const layoutPh = ph ? matchPlaceholder(ctx.layoutPh, ph) : undefined;
    const masterPh = ph ? matchPlaceholder(ctx.masterPh, ph) : undefined;
    const ownXfrm = xfrmOf(name === "graphicFrame" ? kid(el, "xfrm") : path(el, "spPr", "xfrm"));
    const rect = ownXfrm ?? layoutPh?.xfrm ?? masterPh?.xfrm;
    if (!rect) continue;
    const box = applyTransform(t, rect);
    const spPr = kid(el, "spPr");
    const bodyPr = path(el, "txBody", "bodyPr");
    const anchorAttr = bodyPr?.getAttribute("anchor") ?? layoutPh?.anchor ?? masterPh?.anchor;

    const base: PptxShape = {
      key: shapeKey,
      type: "shape",
      ...box,
      rot: ownXfrm?.rot ?? 0,
      fill: fillOf(spPr, ctx.theme),
      line: kid(spPr, "ln") ? colorFrom(path(spPr, "ln", "solidFill"), ctx.theme) : undefined,
      geom: path(spPr, "prstGeom")?.getAttribute("prst") || undefined,
      anchor: anchorAttr === "ctr" ? "middle" : anchorAttr === "b" ? "bottom" : "top",
      insets: [
        num(bodyPr?.getAttribute("lIns"), 91440),
        num(bodyPr?.getAttribute("tIns"), 45720),
        num(bodyPr?.getAttribute("rIns"), 91440),
        num(bodyPr?.getAttribute("bIns"), 45720),
      ],
      paragraphs: [],
      placeholder: ph?.type,
    };

    if (name === "sp") {
      base.paragraphs = parseTextBody(kid(el, "txBody"), ctx, shapeKey, ph, layoutPh, masterPh);
      base.type = base.paragraphs.length > 0 ? "text" : "shape";
      shapes.push(base);
    } else if (name === "pic") {
      base.type = "image";
      const blip = path(el, "blipFill", "blip");
      const rel = blip ? ctx.rels.get(rAttr(blip, "embed") || "") : undefined;
      if (ctx.withImages && rel && !rel.external) {
        const ext = rel.target.split(".").pop()?.toLowerCase() || "";
        const file = ctx.zip.file(rel.target);
        if (file && IMAGE_MIME[ext]) {
          base.imageSrc = `data:${IMAGE_MIME[ext]};base64,${bytesToBase64(await file.async("uint8array"))}`;
        }
      }
      shapes.push(base);
    } else if (name === "graphicFrame") {
      const tbl = descendants(el, "tbl")[0];
      if (tbl) {
        base.type = "table";
        base.table = {
          colWidths: descendants(tbl, "gridCol").map((g) => num(g.getAttribute("w"))),
          rows: kids(tbl, "tr").map((tr) =>
            kids(tr, "tc").map((tc) =>
              kids(kid(tc, "txBody"), "p")
                .map(paragraphText)
                .join("\n")
            )
          ),
        };
      }
      shapes.push(base);
    } else {
      shapes.push(base);
    }
  }
  return shapes;
}

function backgroundOf(doc: Document | null, theme: Theme): string | undefined {
  const bg = doc ? descendants(doc, "bg")[0] : null;
  if (!bg) return undefined;
  const bgPr = kid(bg, "bgPr");
  if (bgPr) return fillOf(bgPr, theme);
  const ref = kid(bg, "bgRef");
  return ref ? colorFrom(ref, theme) : undefined;
}

export async function parsePptx(
  bytes: Uint8Array,
  options: { withImages?: boolean } = {}
): Promise<PptxSession> {
  const withImages = options.withImages ?? true;
  const zip = await JSZip.loadAsync(bytes);
  const presPath = "ppt/presentation.xml";
  const pres = await readXml(zip, presPath);
  if (!pres) throw new Error("올바른 PowerPoint 파일이 아닙니다 (ppt/presentation.xml 없음).");
  const sldSz = descendants(pres, "sldSz")[0];
  const deck: PptxDeck = {
    width: num(sldSz?.getAttribute("cx"), 12192000),
    height: num(sldSz?.getAttribute("cy"), 6858000),
    slides: [],
  };
  const presRels = await readRels(zip, presPath);
  const slidePaths = descendants(pres, "sldId")
    .map((s) => presRels.get(rAttr(s, "id") || "")?.target)
    .filter((p): p is string => !!p && !!zip.file(p));

  const docs = new Map<string, Document>();
  const paragraphEls = new Map<string, Element>();
  const cache = new Map<string, Document | null>();
  const cachedXml = async (p: string) => {
    if (!cache.has(p)) cache.set(p, await readXml(zip, p));
    return cache.get(p)!;
  };

  for (const [slideIndex, slidePath] of slidePaths.entries()) {
    const slideDoc = (await readXml(zip, slidePath))!;
    docs.set(slidePath, slideDoc);
    const rels = await readRels(zip, slidePath);
    const layoutPath = relByType(rels, "/slideLayout")?.target;
    const layoutDoc = layoutPath ? await cachedXml(layoutPath) : null;
    const masterPath = layoutPath ? relByType(await readRels(zip, layoutPath), "/slideMaster")?.target : undefined;
    const masterDoc = masterPath ? await cachedXml(masterPath) : null;
    const theme = masterPath ? await readTheme(zip, masterPath) : {};

    const ctx: SlideContext = {
      slideIndex,
      theme,
      layoutPh: collectPlaceholders(layoutDoc),
      masterPh: collectPlaceholders(masterDoc),
      styles: masterStyles(masterDoc),
      rels,
      zip,
      paragraphEls,
      counter: { n: 0 },
      withImages,
    };
    const spTree = descendants(slideDoc, "spTree")[0];
    const shapes = spTree ? await parseShapes(spTree, ctx, IDENTITY) : [];

    let notes = "";
    const notesPath = relByType(rels, "/notesSlide")?.target;
    const notesDoc = notesPath ? await readXml(zip, notesPath) : null;
    if (notesDoc) {
      const body = descendants(notesDoc, "sp").find((sp) => phInfo(sp)?.type === "body");
      notes = kids(kid(body, "txBody"), "p").map(paragraphText).join("\n").trim();
    }

    deck.slides.push({
      index: slideIndex,
      path: slidePath,
      shapes,
      background:
        backgroundOf(slideDoc, theme) ?? backgroundOf(layoutDoc, theme) ?? backgroundOf(masterDoc, theme) ?? "#FFFFFF",
      notes,
    });
  }

  return { deck, zip, docs, paragraphEls };
}

// ---------------------------------------------------------------------------
// 편집 반영
// ---------------------------------------------------------------------------

/**
 * 문단 텍스트를 바꾸되 첫 번째 런의 서식(글꼴, 크기, 색)을 유지한다.
 * 텍스트 안의 줄바꿈("\n")은 PowerPoint의 줄 바꿈(<a:br/>)으로 저장한다.
 */
export function setParagraphText(p: Element, text: string): void {
  const doc = p.ownerDocument;
  const end = kid(p, "endParaRPr");
  let template = kids(p, "r")[0] ?? null;
  if (!template) {
    template = doc.createElementNS(A_NS, "a:r");
    const rPr = doc.createElementNS(A_NS, "a:rPr");
    if (end) {
      for (const attr of Array.from(end.attributes)) rPr.setAttribute(attr.name, attr.value);
      for (const c of Array.from(end.children)) rPr.appendChild(c.cloneNode(true));
    }
    template.appendChild(rPr);
  } else {
    template = template.cloneNode(true) as Element;
  }
  kids(p, "r").forEach((r) => r.remove());
  kids(p, "br").forEach((b) => b.remove());
  kids(p, "fld").forEach((f) => f.remove());

  const insert = (node: Element) => (end ? p.insertBefore(node, end) : p.appendChild(node));
  text.split("\n").forEach((segment, i) => {
    if (i > 0) {
      const br = doc.createElementNS(A_NS, "a:br");
      const rPr = kid(template, "rPr");
      if (rPr) br.appendChild(rPr.cloneNode(true));
      insert(br);
    }
    const run = template!.cloneNode(true) as Element;
    let t = kid(run, "t");
    if (!t) {
      t = doc.createElementNS(A_NS, "a:t");
      run.appendChild(t);
    }
    t.textContent = segment;
    insert(run);
  });
}

async function writeZip(zip: JSZip): Promise<Uint8Array> {
  return zip.generateAsync({ type: "uint8array", compression: "DEFLATE" });
}

/** 캔버스에서 바꾼 문단 텍스트(key → text)를 원본 XML에 반영해 새 PPTX 바이트를 만든다. */
export async function applyPptxEdits(session: PptxSession, edits: Map<string, string>): Promise<Uint8Array> {
  const dirty = new Set<string>();
  for (const [key, text] of edits) {
    const p = session.paragraphEls.get(key);
    if (!p) continue;
    if (paragraphText(p) === text) continue;
    setParagraphText(p, text);
    for (const [partPath, doc] of session.docs) {
      if (p.ownerDocument === doc) dirty.add(partPath);
    }
  }
  for (const partPath of dirty) {
    session.zip.file(partPath, serializeXml(session.docs.get(partPath)!));
  }
  return writeZip(session.zip);
}

// ---------------------------------------------------------------------------
// PPTX → Markdown (AI 읽기용)
// ---------------------------------------------------------------------------

export function deckToMarkdown(deck: PptxDeck, edits?: Map<string, string>): string {
  const textOf = (p: PptxParagraph) => (edits?.get(p.key) ?? p.text).replace(/\n/g, " ").trim();
  const out: string[] = [];
  for (const slide of deck.slides) {
    const shapes = [...slide.shapes].sort((a, b) => a.y - b.y || a.x - b.x);
    const titleShape = shapes.find((s) => s.placeholder && TITLE_TYPES.includes(s.placeholder));
    const title = titleShape ? titleShape.paragraphs.map(textOf).filter(Boolean).join(" ") : "";
    out.push(`## 슬라이드 ${slide.index + 1}${title ? `: ${title}` : ""}`);
    for (const s of shapes) {
      if (s === titleShape) continue;
      if (s.type === "text") {
        for (const p of s.paragraphs) {
          const t = textOf(p);
          if (!t) continue;
          out.push(p.bullet ? `${"  ".repeat(p.level)}- ${t}` : t);
        }
      } else if (s.type === "table" && s.table && s.table.rows.length > 0) {
        const rows = s.table.rows.map((r) => r.map((c) => c.replace(/\n/g, " ").replace(/\|/g, "\\|")));
        out.push("", `| ${rows[0].join(" | ")} |`, `| ${rows[0].map(() => "---").join(" | ")} |`);
        rows.slice(1).forEach((r) => out.push(`| ${r.join(" | ")} |`));
        out.push("");
      } else if (s.type === "image") {
        out.push("[이미지]");
      }
    }
    if (slide.notes) out.push(`> 발표자 노트: ${slide.notes.replace(/\n/g, " ")}`);
    out.push("");
  }
  return out.join("\n").trim() + "\n";
}

export async function pptxToMarkdown(bytes: Uint8Array): Promise<string> {
  const session = await parsePptx(bytes, { withImages: false });
  return deckToMarkdown(session.deck);
}

// ---------------------------------------------------------------------------
// 서식 유지 텍스트 교체 (DOCX / PPTX 공통, AI의 modify_file용)
// ---------------------------------------------------------------------------

const W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

function nearestParagraph(el: Element, ns: string): Element | null {
  let cur = el.parentElement;
  while (cur) {
    if (cur.localName === "p" && cur.namespaceURI === ns) return cur;
    cur = cur.parentElement;
  }
  return null;
}

/** 문단 안 텍스트 노드들을 이어 붙인 문자열에서 target을 찾아, 걸쳐 있는 노드들만 고친다. */
function replaceInParagraph(p: Element, ns: string, target: string, replacement: string): boolean {
  const nodes = descendants(p, "t").filter((t) => t.namespaceURI === ns && nearestParagraph(t, ns) === p);
  const texts = nodes.map((n) => n.textContent ?? "");
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
        node.textContent = head + replacement + text.slice(end - s);
        state = "after";
      } else {
        node.textContent = head + replacement;
        state = "inside";
      }
    } else if (state === "inside") {
      if (end <= e) {
        node.textContent = text.slice(end - s);
        state = "after";
      } else {
        node.textContent = "";
      }
    } else {
      return;
    }
    if (ns === W_NS) setXmlSpacePreserve(node);
  });
  return state !== "before";
}

function paragraphsIn(doc: Document, ns: string): Element[] {
  return descendants(doc, "p").filter((p) => p.namespaceURI === ns);
}

/**
 * DOCX/PPTX 안에서 target 문자열을 찾아 replacement로 바꾼다. 서식(런 속성)은 유지된다.
 * 대상은 한 문단 안에 있어야 한다. 찾지 못하면 오류를 던진다.
 */
export async function replaceTextInOoxml(
  bytes: Uint8Array,
  kind: "docx" | "pptx",
  target: string,
  replacement: string
): Promise<Uint8Array> {
  if (!target) throw new Error("target이 비어 있습니다.");
  const zip = await JSZip.loadAsync(bytes);
  let parts: string[];
  let ns: string;
  if (kind === "docx") {
    ns = W_NS;
    parts = ["word/document.xml"].concat(
      Object.keys(zip.files).filter((f) => /^word\/(header|footer)\d*\.xml$/.test(f))
    );
  } else {
    ns = A_NS;
    const pres = await readXml(zip, "ppt/presentation.xml");
    const rels = await readRels(zip, "ppt/presentation.xml");
    parts = pres
      ? descendants(pres, "sldId")
          .map((s) => rels.get(rAttr(s, "id") || "")?.target)
          .filter((p): p is string => !!p)
      : [];
  }

  for (const part of parts) {
    const doc = await readXml(zip, part);
    if (!doc) continue;
    for (const p of paragraphsIn(doc, ns)) {
      if (replaceInParagraph(p, ns, target, replacement)) {
        zip.file(part, serializeXml(doc));
        return writeZip(zip);
      }
    }
  }
  throw new Error(
    "문서에서 대상 문자열(target)을 한 문단 안에서 찾지 못했습니다. read_file로 현재 내용을 확인하고, 한 문단 안의 정확한 문구를 지정하세요."
  );
}

// ---------------------------------------------------------------------------
// Markdown → PPTX (AI 생성용)
// ---------------------------------------------------------------------------

interface InlineRun {
  text: string;
  bold?: boolean;
  italic?: boolean;
  code?: boolean;
}

type SlideBlock =
  | { type: "para"; runs: InlineRun[]; heading?: boolean; quote?: boolean }
  | { type: "bullet"; runs: InlineRun[]; level: number; ordered: boolean }
  | { type: "table"; rows: string[][] }
  | { type: "code"; text: string };

interface SlideSpec {
  title: string;
  blocks: SlideBlock[];
  notes: string[];
}

function inlineRuns(tokens: Token[] | undefined, fmt: Omit<InlineRun, "text"> = {}): InlineRun[] {
  const out: InlineRun[] = [];
  for (const t of tokens ?? []) {
    switch (t.type) {
      case "strong":
        out.push(...inlineRuns((t as Tokens.Strong).tokens, { ...fmt, bold: true }));
        break;
      case "em":
        out.push(...inlineRuns((t as Tokens.Em).tokens, { ...fmt, italic: true }));
        break;
      case "codespan":
        out.push({ ...fmt, code: true, text: (t as Tokens.Codespan).text });
        break;
      case "del":
      case "link":
        out.push(...inlineRuns((t as Tokens.Link).tokens, fmt));
        break;
      case "br":
        out.push({ ...fmt, text: " " });
        break;
      case "text": {
        const tt = t as Tokens.Text;
        if (tt.tokens && tt.tokens.length) out.push(...inlineRuns(tt.tokens, fmt));
        else out.push({ ...fmt, text: decodeEntities(tt.text) });
        break;
      }
      case "escape":
        out.push({ ...fmt, text: (t as Tokens.Escape).text });
        break;
      case "html":
        out.push({ ...fmt, text: (t as Tokens.HTML).text.replace(/<[^>]+>/g, "") });
        break;
      default:
        if ("text" in t && typeof t.text === "string") out.push({ ...fmt, text: decodeEntities(t.text) });
    }
  }
  return out.filter((r) => r.text);
}

function decodeEntities(s: string): string {
  return s
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&amp;/g, "&");
}

function runsText(runs: InlineRun[]): string {
  return runs.map((r) => r.text).join("");
}

const NOTE_RE = /^(note|notes|노트|발표자\s*노트)\s*[:：]\s*/i;

function listBlocks(list: Tokens.List, level: number, out: SlideBlock[]) {
  for (const item of list.items) {
    const textTokens = item.tokens.filter((t) => t.type !== "list");
    const runs = textTokens.flatMap((t) =>
      t.type === "text" || t.type === "paragraph"
        ? inlineRuns((t as Tokens.Text).tokens ?? [{ type: "text", raw: t.raw, text: (t as Tokens.Text).text } as Token])
        : []
    );
    out.push({ type: "bullet", runs, level, ordered: list.ordered });
    item.tokens
      .filter((t): t is Tokens.List => t.type === "list")
      .forEach((nested) => listBlocks(nested, Math.min(level + 1, 4), out));
  }
}

export function markdownToSlideSpecs(md: string): SlideSpec[] {
  const tokens = marked.lexer(md);
  const groups: Token[][] = [];
  const useHr = tokens.some((t) => t.type === "hr");
  let current: Token[] = [];
  for (const t of tokens) {
    const startsNew = useHr ? t.type === "hr" : t.type === "heading" && (t as Tokens.Heading).depth <= 2;
    if (startsNew && current.some((c) => c.type !== "space")) {
      groups.push(current);
      current = [];
    }
    if (!(useHr && t.type === "hr")) current.push(t);
  }
  if (current.some((c) => c.type !== "space")) groups.push(current);

  return groups.map((group) => {
    const spec: SlideSpec = { title: "", blocks: [], notes: [] };
    for (const t of group) {
      switch (t.type) {
        case "heading": {
          const h = t as Tokens.Heading;
          if (!spec.title) spec.title = runsText(inlineRuns(h.tokens));
          else spec.blocks.push({ type: "para", runs: inlineRuns(h.tokens), heading: true });
          break;
        }
        case "paragraph": {
          const p = t as Tokens.Paragraph;
          if (NOTE_RE.test(p.text)) spec.notes.push(p.text.replace(NOTE_RE, ""));
          else spec.blocks.push({ type: "para", runs: inlineRuns(p.tokens) });
          break;
        }
        case "list":
          listBlocks(t as Tokens.List, 0, spec.blocks);
          break;
        case "table": {
          const tb = t as Tokens.Table;
          spec.blocks.push({
            type: "table",
            rows: [
              tb.header.map((c) => runsText(inlineRuns(c.tokens))),
              ...tb.rows.map((r) => r.map((c) => runsText(inlineRuns(c.tokens)))),
            ],
          });
          break;
        }
        case "code":
          spec.blocks.push({ type: "code", text: (t as Tokens.Code).text });
          break;
        case "blockquote": {
          const bq = t as Tokens.Blockquote;
          const text = bq.text.trim();
          if (NOTE_RE.test(text)) spec.notes.push(text.replace(NOTE_RE, ""));
          else
            bq.tokens
              .filter((x): x is Tokens.Paragraph => x.type === "paragraph")
              .forEach((x) => spec.blocks.push({ type: "para", runs: inlineRuns(x.tokens), quote: true }));
          break;
        }
        default:
          break;
      }
    }
    return spec;
  });
}

const SLIDE_W = 13.333;
const SLIDE_H = 7.5;
const FONT = "Malgun Gothic";
const ACCENT = "A50034";

function estimateLines(text: string, fontPt: number, widthIn: number): number {
  const charW = (fontPt / 72) * 0.95;
  const perLine = Math.max(8, Math.floor(widthIn / charW));
  return text.split("\n").reduce((n, line) => n + Math.max(1, Math.ceil(line.length / perLine)), 0);
}

type PptxTextProps = PptxGenJS.TextProps;

function toTextProps(blocks: SlideBlock[], fontSize: number): PptxTextProps[] {
  const out: PptxTextProps[] = [];
  for (const b of blocks) {
    if (b.type === "table") continue;
    const runs: InlineRun[] = b.type === "code" ? [{ text: b.text, code: true }] : b.runs;
    const safeRuns = runs.length ? runs : [{ text: " " }];
    safeRuns.forEach((r, i) => {
      const isFirst = i === 0;
      const isLast = i === safeRuns.length - 1;
      const options: PptxGenJS.TextPropsOptions = {
        bold: r.bold || (b.type === "para" && b.heading) || undefined,
        italic: r.italic || (b.type === "para" && b.quote) || undefined,
        fontFace: r.code || b.type === "code" ? "Consolas" : FONT,
        fontSize: b.type === "para" && b.heading ? fontSize + 2 : b.type === "code" ? fontSize - 3 : fontSize,
        color: b.type === "para" && b.quote ? "666666" : "333333",
        breakLine: isLast,
      };
      if (isFirst) {
        if (b.type === "bullet") {
          options.bullet = b.ordered ? { type: "number" } : { code: "2022" };
          options.indentLevel = b.level;
        }
        options.paraSpaceAfter = 6;
      }
      out.push({ text: r.text, options });
    });
  }
  return out;
}

export async function markdownToPptx(md: string, title = "프레젠테이션"): Promise<Uint8Array> {
  const specs = markdownToSlideSpecs(md);
  if (specs.length === 0) specs.push({ title, blocks: [], notes: [] });

  const pptx = new PptxGenJS();
  pptx.layout = "LAYOUT_WIDE";
  pptx.title = title;
  pptx.author = "Desktop App for LG K-EXAONE";
  pptx.theme = { headFontFace: FONT, bodyFontFace: FONT };

  specs.forEach((spec, idx) => {
    const slide = pptx.addSlide();
    slide.background = { color: "FFFFFF" };
    const textBlocks = spec.blocks.filter((b) => b.type !== "table");
    const tables = spec.blocks.filter((b): b is Extract<SlideBlock, { type: "table" }> => b.type === "table");
    const isTitleSlide =
      idx === 0 && tables.length === 0 && textBlocks.every((b) => b.type === "para") && textBlocks.length <= 2;

    if (isTitleSlide) {
      slide.addShape(pptx.ShapeType.rect, { x: 0, y: 0, w: 0.25, h: SLIDE_H, fill: { color: ACCENT }, line: { color: ACCENT } });
      slide.addText(spec.title || title, {
        x: 0.9, y: 2.4, w: SLIDE_W - 1.8, h: 1.5, fontFace: FONT, fontSize: 40, bold: true, color: "1F1E1D", valign: "bottom",
      });
      if (textBlocks.length > 0) {
        slide.addText(
          textBlocks.map((b) => runsText((b as { runs: InlineRun[] }).runs)).join("\n"),
          { x: 0.9, y: 4.0, w: SLIDE_W - 1.8, h: 1.2, fontFace: FONT, fontSize: 20, color: "666666", valign: "top" }
        );
      }
    } else {
      if (spec.title) {
        slide.addText(spec.title, {
          x: 0.6, y: 0.35, w: SLIDE_W - 1.2, h: 0.9, fontFace: FONT, fontSize: 28, bold: true, color: "1F1E1D", valign: "middle",
        });
        slide.addShape(pptx.ShapeType.rect, { x: 0.6, y: 1.25, w: 1.2, h: 0.06, fill: { color: ACCENT }, line: { color: ACCENT } });
      }
      let y = spec.title ? 1.55 : 0.6;
      const bottom = SLIDE_H - 0.6;
      const width = SLIDE_W - 1.2;
      const tableHeights = tables.map((t) => t.rows.length * 0.42 + 0.1);
      const availableForText = Math.max(1, bottom - y - tableHeights.reduce((a, b) => a + b + 0.2, 0));

      if (textBlocks.length > 0) {
        let fontSize = 18;
        const plain = textBlocks
          .map((b) => (b.type === "code" ? b.text : "  ".repeat(b.type === "bullet" ? b.level + 1 : 0) + runsText(b.runs)))
          .join("\n");
        while (fontSize > 11 && estimateLines(plain, fontSize, width) * (fontSize / 72) * 1.45 > availableForText) {
          fontSize--;
        }
        const h = Math.min(availableForText, estimateLines(plain, fontSize, width) * (fontSize / 72) * 1.45 + 0.2);
        slide.addText(toTextProps(textBlocks, fontSize), { x: 0.6, y, w: width, h, valign: "top", fontFace: FONT });
        y += h + 0.2;
      }

      tables.forEach((t, i) => {
        const rows: PptxGenJS.TableRow[] = t.rows.map((r, ri) =>
          r.map((cell) => ({
            text: cell,
            options: ri === 0 ? { bold: true, fill: { color: "F2F2F2" }, color: "1F1E1D" } : { color: "333333" },
          }))
        );
        slide.addTable(rows, {
          x: 0.6, y, w: width, fontFace: FONT, fontSize: 12, border: { type: "solid", pt: 0.75, color: "BFBFBF" }, autoPage: false,
        });
        y += tableHeights[i] + 0.2;
      });
    }

    slide.slideNumber = { x: SLIDE_W - 1.0, y: SLIDE_H - 0.5, w: 0.6, h: 0.3, fontSize: 10, color: "999999" };
    if (spec.notes.length > 0) slide.addNotes(spec.notes.join("\n"));
  });

  const out = await pptx.write({ outputType: "uint8array" });
  return out instanceof Uint8Array ? out : new Uint8Array(out as ArrayBuffer);
}
