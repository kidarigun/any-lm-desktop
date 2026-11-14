/**
 * 엑셀(.xlsx) ↔ Markdown 변환.
 *
 * AI가 엑셀 파일을 Python 없이 바로 만들고(write_file), 읽고(read_file), 셀 값을 고칠(modify_file) 수 있게 한다.
 *
 * Markdown → xlsx 규칙
 * - 제목(#, ## …)마다 시트 하나. 제목 앞의 표는 첫 시트에 들어간다.
 * - 표의 머리글 줄이 시트의 1행, 데이터는 2행부터 (표 앞에 글이 있으면 그만큼 아래로 밀린다).
 * - 한 시트에 표가 여러 개면 빈 줄 하나를 두고 이어 붙인다.
 * - 칸 값: 숫자(1,200 → 천 단위 구분 유지)·백분율(12.5%)·날짜(2026-01-31)는 엑셀 값으로, '='로 시작하면 수식.
 * - ```csv 코드 블록도 표로 읽는다.
 */
import JSZip from "jszip";

const NS = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_REL = "http://schemas.openxmlformats.org/package/2006/relationships";

const MAX_SHEET_NAME = 31;
/** 읽을 때 시트마다 보여 줄 최대 행·열 (너무 크면 모델 입력이 넘친다) */
const READ_MAX_ROWS = 1000;
const READ_MAX_COLS = 50;

// ---------------------------------------------------------------------------
// 칸 값
// ---------------------------------------------------------------------------

export type CellValue =
  | { t: "s"; v: string; bold?: boolean }
  | { t: "n"; v: number; style: NumberStyle }
  | { t: "f"; f: string };

type NumberStyle = "general" | "thousands" | "thousands2" | "percent" | "percent1" | "date";

/** 칸 안의 Markdown 기호만 걷어 낸다 (snake_case 같은 밑줄은 그대로 둔다). */
function cellText(raw: string): { text: string; bold: boolean } {
  let s = raw.trim();
  const bold = /^(\*\*|__)(.+)\1$/.test(s);
  s = s
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/(\*\*|__)(.+?)\1/g, "$2")
    .replace(/~~(.+?)~~/g, "$1")
    .replace(/`([^`]+)`/g, "$1");
  return { text: s, bold };
}

const EXCEL_EPOCH = Date.UTC(1899, 11, 30);

function dateSerial(y: number, m: number, d: number): number | null {
  const t = Date.UTC(y, m - 1, d);
  const dt = new Date(t);
  if (dt.getUTCFullYear() !== y || dt.getUTCMonth() !== m - 1 || dt.getUTCDate() !== d) return null;
  return (t - EXCEL_EPOCH) / 86_400_000;
}

/** Markdown 표 칸 하나를 엑셀 값으로. 빈 칸이면 null */
export function parseCell(raw: string): CellValue | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  // 수식: =SUM(B2:B13) (Markdown 기호를 걷지 않는다: * 는 곱셈)
  if (/^=\S/.test(trimmed)) return { t: "f", f: trimmed.slice(1).replace(/\\\|/g, "|") };

  const { text, bold } = cellText(trimmed);
  if (!bold) {
    // 천 단위 구분 기호가 맞게 들어간 숫자이거나, 구분 기호가 없는 숫자
    const withCommas = /^[-+]?\d{1,3}(,\d{3})+(\.\d+)?$/.test(text);
    const plain = /^[-+]?(\d+(\.\d+)?|\.\d+)([eE][-+]?\d+)?$/.test(text);
    // 0으로 시작하는 코드(007, 010-…)와 15자리 넘는 번호는 글자로 둔다.
    const looksLikeCode = /^[-+]?0\d/.test(text) || text.replace(/\D/g, "").length > 15;
    if ((withCommas || plain) && !looksLikeCode) {
      const v = Number(text.replace(/,/g, ""));
      if (Number.isFinite(v)) {
        const style: NumberStyle = withCommas ? (text.includes(".") ? "thousands2" : "thousands") : "general";
        return { t: "n", v, style };
      }
    }
    const pct = /^([-+]?\d+(\.\d+)?)%$/.exec(text);
    if (pct) return { t: "n", v: Number(pct[1]) / 100, style: pct[2] ? "percent1" : "percent" };
    const date = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text);
    if (date) {
      const serial = dateSerial(Number(date[1]), Number(date[2]), Number(date[3]));
      if (serial !== null) return { t: "n", v: serial, style: "date" };
    }
  }
  return { t: "s", v: text, bold };
}

// ---------------------------------------------------------------------------
// Markdown → 시트
// ---------------------------------------------------------------------------

export interface ParsedSheet {
  name: string;
  rows: string[][];
  /** 머리글 줄의 행 번호 (0부터) */
  headerRows: Set<number>;
}

function splitTableRow(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|") && !s.endsWith("\\|")) s = s.slice(0, -1);
  const cells: string[] = [];
  let cur = "";
  for (let i = 0; i < s.length; i++) {
    if (s[i] === "\\" && s[i + 1] === "|") {
      cur += "|";
      i++;
    } else if (s[i] === "|") {
      cells.push(cur.trim());
      cur = "";
    } else {
      cur += s[i];
    }
  }
  cells.push(cur.trim());
  return cells;
}

const isTableLine = (line: string) => /^\s*\|.*\|/.test(line);
const isSeparatorRow = (line: string) => /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/.test(line) && line.includes("-");

function parseDelimited(lines: string[], delimiter: string): string[][] {
  const rows: string[][] = [];
  const text = lines.join("\n");
  let row: string[] = [];
  let cur = "";
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (quoted) {
      if (ch === '"' && text[i + 1] === '"') {
        cur += '"';
        i++;
      } else if (ch === '"') quoted = false;
      else cur += ch;
    } else if (ch === '"' && cur === "") quoted = true;
    else if (ch === delimiter) {
      row.push(cur);
      cur = "";
    } else if (ch === "\n") {
      row.push(cur);
      rows.push(row);
      row = [];
      cur = "";
    } else cur += ch;
  }
  if (cur !== "" || row.length) {
    row.push(cur);
    rows.push(row);
  }
  return rows.filter((r) => r.some((c) => c.trim() !== ""));
}

export function sanitizeSheetName(name: string, used: Set<string>): string {
  const base =
    cellText(name)
      .text.replace(/[[\]:*?/\\]/g, " ")
      .replace(/\s+/g, " ")
      .trim()
      .replace(/^'+|'+$/g, "")
      .slice(0, MAX_SHEET_NAME)
      .trim() || "Sheet";
  let candidate = base;
  for (let i = 2; used.has(candidate.toLowerCase()); i++) {
    const suffix = ` (${i})`;
    candidate = base.slice(0, MAX_SHEET_NAME - suffix.length) + suffix;
  }
  used.add(candidate.toLowerCase());
  return candidate;
}

export function markdownToSheets(markdown: string, fallbackName = "Sheet1"): ParsedSheet[] {
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  const sheets: ParsedSheet[] = [];
  let current: ParsedSheet | null = null;
  const start = (name: string) => {
    current = { name, rows: [], headerRows: new Set() };
    sheets.push(current);
    return current;
  };
  const sheet = (): ParsedSheet => current ?? start(fallbackName);
  const addBlock = (rows: string[][], hasHeader: boolean) => {
    const s = sheet();
    if (s.rows.length > 0) s.rows.push([]);
    if (hasHeader) s.headerRows.add(s.rows.length);
    s.rows.push(...rows);
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const heading = /^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$/.exec(line);
    if (heading) {
      start(heading[1]);
      continue;
    }
    const fence = /^\s*(```|~~~)\s*([\w-]*)/.exec(line);
    if (fence) {
      const body: string[] = [];
      for (i++; i < lines.length && !lines[i].trim().startsWith(fence[1]); i++) body.push(lines[i]);
      const lang = fence[2].toLowerCase();
      if (lang === "csv" || lang === "tsv") {
        const rows = parseDelimited(body, lang === "tsv" ? "\t" : ",");
        if (rows.length) addBlock(rows, true);
      } else {
        for (const b of body) if (b.trim()) sheet().rows.push([b]);
      }
      continue;
    }
    if (isTableLine(line)) {
      const block: string[][] = [];
      const hasHeader = i + 1 < lines.length && isSeparatorRow(lines[i + 1]);
      block.push(splitTableRow(line));
      if (hasHeader) i++;
      while (i + 1 < lines.length && isTableLine(lines[i + 1])) {
        i++;
        if (!isSeparatorRow(lines[i])) block.push(splitTableRow(lines[i]));
      }
      addBlock(block, hasHeader);
      continue;
    }
    const text = line.trim();
    if (!text || /^(-{3,}|\*{3,}|_{3,})$/.test(text)) continue;
    // 표 밖의 글은 첫 칸에 한 줄씩 (목록 기호와 인용 기호는 뗀다)
    sheet().rows.push([text.replace(/^(?:[-*+]|\d+[.)]|>)\s+/, "")]);
  }

  const used = new Set<string>();
  const nonEmpty = sheets.filter((s) => s.rows.length > 0);
  const result = nonEmpty.length ? nonEmpty : [{ name: fallbackName, rows: [], headerRows: new Set<number>() }];
  for (const s of result) s.name = sanitizeSheetName(s.name, used);
  return result;
}

// ---------------------------------------------------------------------------
// 시트 → xlsx
// ---------------------------------------------------------------------------

const STYLE_INDEX = {
  general: 0,
  header: 1,
  thousands: 2,
  thousands2: 3,
  percent1: 4,
  date: 5,
  bold: 6,
  percent: 7,
} as const;

const STYLES_XML = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="${NS}">
<numFmts count="2"><numFmt numFmtId="164" formatCode="yyyy-mm-dd"/><numFmt numFmtId="165" formatCode="0.0%"/></numFmts>
<fonts count="2"><font><sz val="11"/><name val="맑은 고딕"/><family val="2"/></font><font><b/><sz val="11"/><name val="맑은 고딕"/><family val="2"/></font></fonts>
<fills count="3"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill><fill><patternFill patternType="solid"><fgColor rgb="FFF2F2F2"/><bgColor indexed="64"/></patternFill></fill></fills>
<borders count="2"><border><left/><right/><top/><bottom/><diagonal/></border><border><left/><right/><top/><bottom style="thin"><color rgb="FF999999"/></bottom><diagonal/></border></borders>
<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>
<cellXfs count="8">
<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>
<xf numFmtId="0" fontId="1" fillId="2" borderId="1" xfId="0" applyFont="1" applyFill="1" applyBorder="1"/>
<xf numFmtId="3" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>
<xf numFmtId="4" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>
<xf numFmtId="165" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>
<xf numFmtId="164" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>
<xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/>
<xf numFmtId="9" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>
</cellXfs>
<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>
</styleSheet>`;

export function escapeXml(s: string): string {
  return s
    .replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F]/g, "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export function columnName(index: number): string {
  let n = index + 1;
  let s = "";
  while (n > 0) {
    const r = (n - 1) % 26;
    s = String.fromCharCode(65 + r) + s;
    n = Math.floor((n - 1) / 26);
  }
  return s;
}

export function columnIndex(name: string): number {
  let n = 0;
  for (const ch of name.toUpperCase()) n = n * 26 + (ch.charCodeAt(0) - 64);
  return n - 1;
}

/** 글자 폭: 한글·한자 등 넓은 글자는 2칸 */
function displayWidth(s: string): number {
  let w = 0;
  for (const ch of s) w += /[ᄀ-ᇿ⺀-꓏가-힣豈-﫿︰-﹏＀-｠￠-￦]/.test(ch) ? 2 : 1;
  return w;
}

function inlineString(text: string): string {
  return `<is><t xml:space="preserve">${escapeXml(text)}</t></is>`;
}

function sheetXml(sheet: ParsedSheet): string {
  const widths: number[] = [];
  const rowsXml: string[] = [];
  sheet.rows.forEach((row, r) => {
    const isHeader = sheet.headerRows.has(r);
    const cells: string[] = [];
    row.forEach((raw, c) => {
      const cell = parseCell(raw);
      if (!cell) return;
      const ref = `${columnName(c)}${r + 1}`;
      let shown = raw.trim();
      if (cell.t === "f") {
        cells.push(`<c r="${ref}"${isHeader ? ` s="${STYLE_INDEX.header}"` : ""}><f>${escapeXml(cell.f)}</f></c>`);
        shown = "0000000000";
      } else if (cell.t === "n") {
        const s = isHeader ? STYLE_INDEX.header : STYLE_INDEX[cell.style];
        cells.push(`<c r="${ref}"${s ? ` s="${s}"` : ""}><v>${cell.v}</v></c>`);
      } else {
        const s = isHeader ? STYLE_INDEX.header : cell.bold ? STYLE_INDEX.bold : 0;
        cells.push(`<c r="${ref}" t="inlineStr"${s ? ` s="${s}"` : ""}>${inlineString(cell.v)}</c>`);
        shown = cell.v;
      }
      // 표 밖의 긴 문장(첫 칸에 한 줄)은 열 너비 계산에서 뺀다.
      if (row.length > 1 || isHeader) {
        const w = Math.max(...shown.split("\n").map(displayWidth));
        widths[c] = Math.max(widths[c] ?? 0, w);
      }
    });
    if (cells.length) rowsXml.push(`<row r="${r + 1}">${cells.join("")}</row>`);
  });

  const cols = widths
    .map((w, i) => (w ? `<col min="${i + 1}" max="${i + 1}" width="${Math.min(60, Math.max(8, w + 2))}" customWidth="1"/>` : ""))
    .join("");
  // 첫 행이 머리글이면 스크롤해도 보이도록 고정한다.
  const pane = sheet.headerRows.has(0)
    ? `<pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/><selection pane="bottomLeft" activeCell="A2" sqref="A2"/>`
    : "";
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="${NS}" xmlns:r="${NS_R}"><sheetViews><sheetView workbookViewId="0">${pane}</sheetView></sheetViews><sheetFormatPr defaultRowHeight="16.5"/>${
    cols ? `<cols>${cols}</cols>` : ""
  }<sheetData>${rowsXml.join("")}</sheetData></worksheet>`;
}

export async function markdownToXlsx(markdown: string, title = "통합 문서"): Promise<Uint8Array> {
  const sheets = markdownToSheets(markdown, "Sheet1");
  const zip = new JSZip();
  const sheetOverrides = sheets
    .map((_, i) => `<Override PartName="/xl/worksheets/sheet${i + 1}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>`)
    .join("");
  zip.file(
    "[Content_Types].xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>${sheetOverrides}<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/></Types>`
  );
  zip.file(
    "_rels/.rels",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="${NS_PKG_REL}"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>`
  );
  const now = new Date().toISOString().replace(/\.\d+Z$/, "Z");
  zip.file(
    "docProps/core.xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>${escapeXml(title)}</dc:title><dc:creator>Desktop App for LG K-EXAONE</dc:creator><dcterms:created xsi:type="dcterms:W3CDTF">${now}</dcterms:created><dcterms:modified xsi:type="dcterms:W3CDTF">${now}</dcterms:modified></cp:coreProperties>`
  );
  zip.file(
    "docProps/app.xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Desktop App for LG K-EXAONE</Application></Properties>`
  );
  zip.file(
    "xl/workbook.xml",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="${NS}" xmlns:r="${NS_R}"><bookViews><workbookView/></bookViews><sheets>${sheets
      .map((s, i) => `<sheet name="${escapeXml(s.name)}" sheetId="${i + 1}" r:id="rId${i + 1}"/>`)
      .join("")}</sheets><calcPr calcId="191029" fullCalcOnLoad="1"/></workbook>`
  );
  zip.file(
    "xl/_rels/workbook.xml.rels",
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="${NS_PKG_REL}">${sheets
      .map(
        (_, i) =>
          `<Relationship Id="rId${i + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet${i + 1}.xml"/>`
      )
      .join("")}<Relationship Id="rId${sheets.length + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>`
  );
  zip.file("xl/styles.xml", STYLES_XML);
  sheets.forEach((s, i) => zip.file(`xl/worksheets/sheet${i + 1}.xml`, sheetXml(s)));
  return zip.generateAsync({ type: "uint8array", compression: "DEFLATE" });
}

// ---------------------------------------------------------------------------
// xlsx → Markdown (읽기)
// ---------------------------------------------------------------------------

function parseXml(text: string): Document {
  const doc = new DOMParser().parseFromString(text, "application/xml");
  if (doc.getElementsByTagName("parsererror").length) throw new Error("엑셀 파일의 XML을 읽지 못했습니다.");
  return doc;
}

async function readXml(zip: JSZip, path: string): Promise<Document | null> {
  const f = zip.file(path);
  return f ? parseXml(await f.async("string")) : null;
}

function children(el: Element | Document, local: string): Element[] {
  return Array.from(el.getElementsByTagNameNS(NS, local));
}

/** <si>·<is> 안의 글자 (발음 표기 rPh는 뺀다) */
function richText(el: Element): string {
  return children(el, "t")
    .filter((t) => (t.parentElement as Element | null)?.localName !== "rPh")
    .map((t) => t.textContent ?? "")
    .join("");
}

function resolveTarget(base: string, target: string): string {
  if (target.startsWith("/")) return target.slice(1);
  const parts = base.split("/").slice(0, -1);
  for (const seg of target.split("/")) {
    if (seg === "..") parts.pop();
    else if (seg !== ".") parts.push(seg);
  }
  return parts.join("/");
}

interface SheetRef {
  name: string;
  path: string;
}

async function workbookSheets(zip: JSZip): Promise<SheetRef[]> {
  const wb = await readXml(zip, "xl/workbook.xml");
  if (!wb) throw new Error("엑셀 통합 문서(xl/workbook.xml)가 없습니다.");
  const rels = await readXml(zip, "xl/_rels/workbook.xml.rels");
  const targets = new Map<string, string>();
  if (rels) {
    for (const r of Array.from(rels.getElementsByTagNameNS(NS_PKG_REL, "Relationship"))) {
      targets.set(r.getAttribute("Id") ?? "", resolveTarget("xl/workbook.xml", r.getAttribute("Target") ?? ""));
    }
  }
  return children(wb, "sheet")
    .map((s) => ({ name: s.getAttribute("name") ?? "Sheet", path: targets.get(s.getAttributeNS(NS_R, "id") ?? "") ?? "" }))
    .filter((s) => s.path && zip.file(s.path));
}

const BUILTIN_DATE_FORMATS = new Set([14, 15, 16, 17, 18, 19, 20, 21, 22, 45, 46, 47]);
const BUILTIN_PERCENT_FORMATS = new Set([9, 10]);
const BUILTIN_THOUSANDS_FORMATS = new Set([3, 4, 37, 38, 39, 40]);

/** 셀 서식 번호 → 보여 줄 숫자 형식 (날짜·백분율·천 단위 구분) */
type CellFormat = "date" | "percent" | "thousands";

async function cellFormats(zip: JSZip): Promise<Map<number, CellFormat>> {
  const styles = await readXml(zip, "xl/styles.xml");
  const out = new Map<number, CellFormat>();
  if (!styles) return out;
  const custom = new Map<number, string>();
  for (const f of children(styles, "numFmt")) custom.set(Number(f.getAttribute("numFmtId")), f.getAttribute("formatCode") ?? "");
  const cellXfs = children(styles, "cellXfs")[0];
  if (!cellXfs) return out;
  Array.from(cellXfs.children).forEach((xf, i) => {
    const id = Number(xf.getAttribute("numFmtId") ?? 0);
    const code = (custom.get(id) ?? "").replace(/"[^"]*"|\[[^\]]*\]|\\./g, "");
    if (BUILTIN_DATE_FORMATS.has(id) || (custom.has(id) && /[yd]/i.test(code))) out.set(i, "date");
    else if (BUILTIN_PERCENT_FORMATS.has(id) || (custom.has(id) && code.includes("%"))) out.set(i, "percent");
    else if (BUILTIN_THOUSANDS_FORMATS.has(id) || (custom.has(id) && /#,##0/.test(code))) out.set(i, "thousands");
  });
  return out;
}

function serialToDate(serial: number): string {
  const ms = Math.round(serial * 86_400_000) + EXCEL_EPOCH;
  const d = new Date(ms);
  const date = `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, "0")}-${String(d.getUTCDate()).padStart(2, "0")}`;
  const secs = Math.round((serial % 1) * 86_400);
  if (!secs) return date;
  return `${date} ${String(d.getUTCHours()).padStart(2, "0")}:${String(d.getUTCMinutes()).padStart(2, "0")}`;
}

function mdCell(s: string): string {
  return s.replace(/\|/g, "\\|").replace(/\r?\n/g, "<br>");
}

/** 미리보기·Markdown 변환용 칸 */
export interface XlsxCell {
  /** 엑셀 화면에 보이는 값 (수식이면 저장된 계산 결과, 결과가 없으면 '=수식') */
  display: string;
  /** Markdown용 값: 숫자는 저장된 값 그대로(날짜만 YYYY-MM-DD), 글자는 display와 같다 */
  raw: string;
  /** 숫자 칸인가 (미리보기에서 오른쪽 정렬) */
  numeric: boolean;
  formula?: string;
}

export interface XlsxSheetData {
  name: string;
  /** rows[행][열] (0부터). 빈 칸은 undefined */
  rows: (XlsxCell | undefined)[][];
  /** 열 수 (마지막으로 값이 있는 열까지) */
  width: number;
  /** 행·열 한도를 넘어 잘렸는가 */
  truncated: boolean;
}

function formatNumber(raw: string, format: CellFormat | undefined): string {
  const n = Number(raw);
  if (raw === "" || !Number.isFinite(n)) return raw;
  if (format === "date") return serialToDate(n);
  if (format === "percent") return `${Number((n * 100).toFixed(4))}%`;
  if (format === "thousands") return n.toLocaleString("en-US", { maximumFractionDigits: 10 });
  return raw;
}

/** 엑셀 파일의 시트를 칸 단위로 읽는다 (미리보기·Markdown 변환 공용). */
export async function readXlsxSheets(
  bytes: Uint8Array,
  limits: { maxRows: number; maxCols: number } = { maxRows: READ_MAX_ROWS, maxCols: READ_MAX_COLS }
): Promise<XlsxSheetData[]> {
  const zip = await JSZip.loadAsync(bytes);
  const shared = await readXml(zip, "xl/sharedStrings.xml");
  const strings = shared ? children(shared, "si").map(richText) : [];
  const formats = await cellFormats(zip);
  const sheets: XlsxSheetData[] = [];

  for (const sheet of await workbookSheets(zip)) {
    const doc = await readXml(zip, sheet.path);
    const rows: (XlsxCell | undefined)[][] = [];
    let maxCol = -1;
    let truncated = false;
    for (const c of doc ? children(doc, "c") : []) {
      const ref = /^([A-Z]+)(\d+)$/.exec(c.getAttribute("r") ?? "");
      if (!ref) continue;
      const col = columnIndex(ref[1]);
      const row = Number(ref[2]) - 1;
      if (row >= limits.maxRows || col >= limits.maxCols) {
        truncated = true;
        continue;
      }
      const f = children(c, "f")[0]?.textContent || undefined;
      const v = children(c, "v")[0]?.textContent ?? "";
      const t = c.getAttribute("t") ?? "n";
      let display: string;
      let raw: string | undefined;
      let numeric = false;
      if (t === "s") display = strings[Number(v)] ?? "";
      else if (t === "inlineStr") display = richText(children(c, "is")[0] ?? c);
      else if (t === "b") display = v === "" ? "" : v === "1" ? "TRUE" : "FALSE";
      else if (t === "str" || t === "e") display = v;
      else {
        const format = formats.get(Number(c.getAttribute("s") ?? 0));
        display = formatNumber(v, format);
        raw = format === "date" ? display : v;
        numeric = v !== "" && format !== "date";
      }
      if (f && display === "") display = `=${f}`;
      if (display === "" && !f) continue;
      (rows[row] ??= [])[col] = { display, raw: raw ?? display, numeric, formula: f };
      maxCol = Math.max(maxCol, col);
    }
    sheets.push({ name: sheet.name, rows: Array.from({ length: rows.length }, (_, r) => rows[r] ?? []), width: maxCol + 1, truncated });
  }
  return sheets;
}

/** 엑셀 파일을 시트별 Markdown 표로. 수식 칸은 '=수식'으로 보여 준다 (AI가 수식을 고칠 수 있게). */
export async function xlsxToMarkdown(bytes: Uint8Array): Promise<string> {
  const out: string[] = [];
  for (const sheet of await readXlsxSheets(bytes)) {
    out.push(`## ${sheet.name}`, "");
    if (sheet.width === 0) {
      out.push("(빈 시트)", "");
      continue;
    }
    const text = (cell: XlsxCell | undefined) => {
      if (!cell) return "";
      if (cell.formula) return `=${cell.formula}`;
      // Markdown에서는 숫자를 원래 값 그대로 (천 단위 구분·백분율 표시는 빼고) 둔다.
      return cell.raw;
    };
    const rows = sheet.rows.map((r) => Array.from({ length: sheet.width }, (_, c) => mdCell(text(r[c]))));
    const line = (cells: string[]) => `| ${cells.join(" | ")} |`;
    out.push(line(rows[0]), line(rows[0].map(() => "---")), ...rows.slice(1).map(line), "");
    if (sheet.truncated) out.push(`(시트가 커서 ${READ_MAX_ROWS}행·${READ_MAX_COLS}열까지만 보여 줍니다.)`, "");
  }
  return out.join("\n").trim() + "\n";
}

// ---------------------------------------------------------------------------
// 셀 값 고치기 (서식 유지)
// ---------------------------------------------------------------------------

function setText(owner: Element, text: string) {
  const doc = owner.ownerDocument;
  while (owner.firstChild) owner.removeChild(owner.firstChild);
  const t = doc.createElementNS(NS, "t");
  t.setAttributeNS("http://www.w3.org/XML/1998/namespace", "xml:space", "preserve");
  t.textContent = text;
  owner.appendChild(t);
}

function numberOf(text: string): number | null {
  const s = text.trim();
  if (!/^[-+]?(\d{1,3}(,\d{3})+|\d+)?(\.\d+)?$/.test(s) || !/\d/.test(s)) return null;
  const v = Number(s.replace(/,/g, ""));
  return Number.isFinite(v) ? v : null;
}

/** 문자열에서 처음 나오는 target 하나만 바꾼다 ($ 같은 치환 기호를 해석하지 않는다). */
function replaceFirst(text: string, target: string, replacement: string): string {
  const i = text.indexOf(target);
  return i < 0 ? text : text.slice(0, i) + replacement + text.slice(i + target.length);
}

/** 칸을 직접 글자를 가진 칸(inlineStr)으로 바꾼다. 서식(s)은 그대로 둔다. */
function makeInlineString(doc: Document, c: Element, text: string) {
  children(c, "v").forEach((v) => v.remove());
  children(c, "is").forEach((is) => is.remove());
  c.setAttribute("t", "inlineStr");
  const is = doc.createElementNS(NS, "is");
  setText(is, text);
  c.appendChild(is);
}

/**
 * 엑셀 파일에서 target을 찾아 replacement로 바꾼다. 다른 파일 형식의 modify_file처럼
 * 시트·칸 순서대로 처음 찾은 한 곳만 바꾸며, 셀 서식·다른 칸은 그대로 둔다.
 * - 글자 칸: 한 칸 안의 문구를 바꾼다 (공유 문자열을 쓰는 칸은 그 칸만 따로 떼어 바꾼다).
 * - '='로 시작하는 target: 수식을 바꾼다.
 * - 숫자 target: 값이 같은 숫자 칸을 바꾼다 (1,200처럼 써도 된다).
 */
export async function replaceTextInXlsx(bytes: Uint8Array, target: string, replacement: string): Promise<Uint8Array> {
  if (!target) throw new Error("target이 비어 있습니다.");
  const zip = await JSZip.loadAsync(bytes);
  const serializer = new XMLSerializer();
  let count = 0;

  const formulaTarget = target.trim().startsWith("=") ? target.trim().slice(1) : null;
  const numTarget = formulaTarget === null ? numberOf(target) : null;
  const numReplacement = numberOf(replacement);
  const shared = await readXml(zip, "xl/sharedStrings.xml");
  const strings = shared ? children(shared, "si").map(richText) : [];

  for (const sheet of await workbookSheets(zip)) {
    const doc = await readXml(zip, sheet.path);
    if (!doc) continue;
    for (const c of children(doc, "c")) {
      const t = c.getAttribute("t") ?? "n";
      const f = children(c, "f")[0];
      if (formulaTarget !== null) {
        if (f?.textContent && f.textContent.includes(formulaTarget)) {
          f.textContent = replaceFirst(f.textContent, formulaTarget, replacement.trim().replace(/^=/, ""));
          children(c, "v").forEach((v) => v.remove());
          count++;
        }
      } else if (t === "s" || t === "inlineStr") {
        const text = t === "s" ? strings[Number(children(c, "v")[0]?.textContent)] ?? "" : richText(children(c, "is")[0] ?? c);
        if (text.includes(target)) {
          makeInlineString(doc, c, replaceFirst(text, target, replacement));
          count++;
        }
      } else if (t === "n" && !f && numTarget !== null) {
        const v = children(c, "v")[0];
        if (v && Number(v.textContent) === numTarget) {
          if (numReplacement !== null) v.textContent = String(numReplacement);
          else makeInlineString(doc, c, replacement);
          count++;
        }
      }
      if (count) break;
    }
    if (count) {
      zip.file(sheet.path, serializer.serializeToString(doc));
      break;
    }
  }

  if (count === 0) {
    throw new Error(
      `엑셀 파일에서 '${target}'을(를) 찾지 못했습니다. read_file로 셀 내용을 확인하고, 한 칸 안의 문구·숫자 값·'='로 시작하는 수식 중 하나를 target으로 지정하세요.`
    );
  }

  // 값이 바뀌었으니 엑셀이 열 때 수식을 다시 계산하게 한다.
  const wb = await readXml(zip, "xl/workbook.xml");
  if (wb) {
    let calc = children(wb, "calcPr")[0];
    if (!calc) {
      calc = wb.createElementNS(NS, "calcPr");
      const after = children(wb, "definedNames")[0] ?? children(wb, "sheets")[0];
      after?.parentNode?.insertBefore(calc, after.nextSibling);
    }
    calc.setAttribute("fullCalcOnLoad", "1");
    zip.file("xl/workbook.xml", serializer.serializeToString(wb));
  }
  return zip.generateAsync({ type: "uint8array", compression: "DEFLATE" });
}
