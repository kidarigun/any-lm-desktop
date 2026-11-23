import mammoth from "mammoth";
import {
  AlignmentType,
  BorderStyle,
  Document,
  ExternalHyperlink,
  HeadingLevel,
  ImageRun,
  LevelFormat,
  Packer,
  Paragraph,
  ShadingType,
  Table,
  TableCell,
  TableRow,
  TextRun,
  WidthType,
  type IParagraphOptions,
  type ParagraphChild,
} from "docx";
import { base64ToBytes, toArrayBuffer } from "./base64";
import { imageSize } from "./imageSize";
import { htmlToMarkdown, markdownToHtml } from "./markdown";

export { imageSize };

const BODY_FONT = "Malgun Gothic";
const CODE_FONT = "Consolas";
const ORDERED_REF = "exaone-ordered";

// ---------------------------------------------------------------------------
// DOCX → HTML / Markdown
// ---------------------------------------------------------------------------

export async function docxToHtml(bytes: Uint8Array): Promise<string> {
  // 브라우저용 mammoth는 arrayBuffer를, Node용은 buffer를 받는다.
  const input = { arrayBuffer: toArrayBuffer(bytes), buffer: bytes } as unknown as {
    arrayBuffer: ArrayBuffer;
  };
  const result = await mammoth.convertToHtml(input, {
    styleMap: ["u => u", "strike => s"],
  });
  return result.value;
}

export async function docxToMarkdown(bytes: Uint8Array): Promise<string> {
  return htmlToMarkdown(await docxToHtml(bytes));
}

// ---------------------------------------------------------------------------
// HTML / Markdown → DOCX
// ---------------------------------------------------------------------------

interface InlineFormat {
  bold?: boolean;
  italics?: boolean;
  underline?: boolean;
  strike?: boolean;
  code?: boolean;
  link?: boolean;
}

interface BlockContext {
  quote: boolean;
}

interface BuildState {
  orderedInstance: number;
}

const HEADINGS: Record<string, (typeof HeadingLevel)[keyof typeof HeadingLevel]> = {
  H1: HeadingLevel.HEADING_1,
  H2: HeadingLevel.HEADING_2,
  H3: HeadingLevel.HEADING_3,
  H4: HeadingLevel.HEADING_4,
  H5: HeadingLevel.HEADING_5,
  H6: HeadingLevel.HEADING_6,
};

function textRun(text: string, fmt: InlineFormat, extra: { break?: number } = {}): TextRun {
  return new TextRun({
    text,
    bold: fmt.bold,
    italics: fmt.italics,
    underline: fmt.underline || fmt.link ? {} : undefined,
    strike: fmt.strike,
    color: fmt.link ? "0563C1" : undefined,
    font: fmt.code ? CODE_FONT : undefined,
    shading: fmt.code ? { type: ShadingType.CLEAR, fill: "F2F2F2", color: "auto" } : undefined,
    ...extra,
  });
}

function imageRun(img: HTMLImageElement): ImageRun | null {
  const src = img.getAttribute("src") || "";
  const m = /^data:image\/(png|jpe?g|gif|bmp);base64,(.+)$/i.exec(src);
  if (!m) return null;
  const type = (m[1].toLowerCase() === "jpeg" ? "jpg" : m[1].toLowerCase()) as "png" | "jpg" | "gif" | "bmp";
  const data = base64ToBytes(m[2]);
  const natural = imageSize(data) ?? { width: 480, height: 320 };
  const attrW = Number(img.getAttribute("width")) || natural.width;
  const attrH = Number(img.getAttribute("height")) || (natural.height * attrW) / natural.width;
  const scale = Math.min(1, 600 / attrW);
  return new ImageRun({
    type,
    data,
    transformation: { width: Math.round(attrW * scale), height: Math.round(attrH * scale) },
  });
}

function inlineChildren(node: Node, fmt: InlineFormat, pre = false): ParagraphChild[] {
  const out: ParagraphChild[] = [];
  node.childNodes.forEach((child) => {
    if (child.nodeType === 3) {
      let text = child.textContent || "";
      if (!pre) text = text.replace(/\s+/g, " ");
      if (text) out.push(textRun(text, fmt));
      return;
    }
    if (child.nodeType !== 1) return;
    const el = child as HTMLElement;
    switch (el.tagName) {
      case "STRONG":
      case "B":
        out.push(...inlineChildren(el, { ...fmt, bold: true }, pre));
        break;
      case "EM":
      case "I":
        out.push(...inlineChildren(el, { ...fmt, italics: true }, pre));
        break;
      case "U":
        out.push(...inlineChildren(el, { ...fmt, underline: true }, pre));
        break;
      case "S":
      case "DEL":
      case "STRIKE":
        out.push(...inlineChildren(el, { ...fmt, strike: true }, pre));
        break;
      case "CODE":
        out.push(...inlineChildren(el, { ...fmt, code: true }, pre));
        break;
      case "BR":
        out.push(textRun("", fmt, { break: 1 }));
        break;
      case "IMG": {
        const run = imageRun(el as HTMLImageElement);
        if (run) out.push(run);
        break;
      }
      case "A": {
        const href = el.getAttribute("href") || "";
        const children = inlineChildren(el, { ...fmt, link: true }, pre);
        if (/^(https?:|mailto:)/i.test(href)) {
          out.push(new ExternalHyperlink({ link: href, children }));
        } else {
          out.push(...children);
        }
        break;
      }
      default:
        out.push(...inlineChildren(el, fmt, pre));
    }
  });
  return out;
}

function quoteProps(ctx: BlockContext): Partial<IParagraphOptions> {
  if (!ctx.quote) return {};
  return {
    indent: { left: 567 },
    border: { left: { style: BorderStyle.SINGLE, size: 18, color: "BFBFBF", space: 8 } },
  };
}

function hasInlineContent(children: ParagraphChild[]): boolean {
  return children.length > 0;
}

function listBlocks(
  list: Element,
  level: number,
  ctx: BlockContext,
  state: BuildState
): (Paragraph | Table)[] {
  const ordered = list.tagName === "OL";
  const instance = ordered ? ++state.orderedInstance : 0;
  const out: (Paragraph | Table)[] = [];
  Array.from(list.children)
    .filter((li) => li.tagName === "LI")
    .forEach((li) => {
      // li 안의 인라인 내용(또는 <p>)을 한 문단으로, 중첩 목록은 다음 수준으로 만든다.
      const inline: ParagraphChild[] = [];
      const nested: Element[] = [];
      li.childNodes.forEach((n) => {
        if (n.nodeType === 1 && ["UL", "OL"].includes((n as Element).tagName)) {
          nested.push(n as Element);
        } else if (n.nodeType === 1 && (n as Element).tagName === "P") {
          if (inline.length > 0) inline.push(textRun("", {}, { break: 1 }));
          inline.push(...inlineChildren(n, {}));
        } else {
          const wrapper = li.ownerDocument.createElement("span");
          wrapper.appendChild(n.cloneNode(true));
          inline.push(...inlineChildren(wrapper, {}));
        }
      });
      out.push(
        new Paragraph({
          children: inline,
          ...(ordered
            ? { numbering: { reference: ORDERED_REF, level, instance } }
            : { bullet: { level } }),
          ...quoteProps(ctx),
        })
      );
      nested.forEach((n) => out.push(...listBlocks(n, Math.min(level + 1, 8), ctx, state)));
    });
  return out;
}

function tableBlock(table: Element, state: BuildState): Table {
  const rows = Array.from(table.querySelectorAll("tr")).filter(
    (tr) => tr.closest("table") === table
  );
  const colCount = Math.max(1, ...rows.map((r) => r.children.length));
  return new Table({
    width: { size: 100, type: WidthType.PERCENTAGE },
    rows: rows.map(
      (tr) =>
        new TableRow({
          // false를 넘기면 <w:tblHeader w:val="off"/>가 생기고, mammoth는 이를 머리글 행으로 읽는다.
          ...(tr === rows[0] && tr.querySelector("th") !== null ? { tableHeader: true } : {}),
          children: Array.from(tr.children).map((cell) => {
            const isHeader = cell.tagName === "TH";
            let children = htmlBlocks(cell, { quote: false }, state).filter(
              (b): b is Paragraph => b instanceof Paragraph
            );
            if (children.length === 0) {
              children = [new Paragraph({ children: inlineChildren(cell, { bold: isHeader }) })];
            }
            return new TableCell({
              children,
              width: { size: Math.floor(100 / colCount), type: WidthType.PERCENTAGE },
              shading: isHeader ? { type: ShadingType.CLEAR, fill: "F2F2F2", color: "auto" } : undefined,
            });
          }),
        })
    ),
  });
}

function htmlBlocks(root: Node, ctx: BlockContext, state: BuildState): (Paragraph | Table)[] {
  const out: (Paragraph | Table)[] = [];
  let pendingInline: ParagraphChild[] = [];
  const flushInline = () => {
    if (hasInlineContent(pendingInline)) {
      out.push(new Paragraph({ children: pendingInline, ...quoteProps(ctx) }));
    }
    pendingInline = [];
  };

  root.childNodes.forEach((child) => {
    if (child.nodeType === 3) {
      const text = (child.textContent || "").replace(/\s+/g, " ");
      if (text.trim()) pendingInline.push(textRun(text, {}));
      return;
    }
    if (child.nodeType !== 1) return;
    const el = child as HTMLElement;
    const tag = el.tagName;

    if (HEADINGS[tag]) {
      flushInline();
      out.push(new Paragraph({ heading: HEADINGS[tag], children: inlineChildren(el, {}) }));
    } else if (tag === "P") {
      flushInline();
      out.push(new Paragraph({ children: inlineChildren(el, {}), ...quoteProps(ctx) }));
    } else if (tag === "UL" || tag === "OL") {
      flushInline();
      out.push(...listBlocks(el, 0, ctx, state));
    } else if (tag === "BLOCKQUOTE") {
      flushInline();
      out.push(...htmlBlocks(el, { quote: true }, state));
    } else if (tag === "PRE") {
      flushInline();
      const lines = (el.textContent || "").replace(/\n$/, "").split("\n");
      lines.forEach((line) =>
        out.push(
          new Paragraph({
            children: [new TextRun({ text: line || " ", font: CODE_FONT, size: 20 })],
            shading: { type: ShadingType.CLEAR, fill: "F5F5F4", color: "auto" },
            spacing: { before: 0, after: 0 },
          })
        )
      );
    } else if (tag === "TABLE") {
      flushInline();
      out.push(tableBlock(el, state));
      out.push(new Paragraph({ children: [] }));
    } else if (tag === "HR") {
      flushInline();
      out.push(
        new Paragraph({
          children: [],
          border: { bottom: { style: BorderStyle.SINGLE, size: 6, color: "BFBFBF", space: 1 } },
        })
      );
    } else if (tag === "IMG") {
      flushInline();
      const run = imageRun(el as HTMLImageElement);
      if (run) out.push(new Paragraph({ children: [run] }));
    } else if (["DIV", "SECTION", "ARTICLE", "MAIN", "BODY", "HEADER", "FOOTER"].includes(tag)) {
      flushInline();
      out.push(...htmlBlocks(el, ctx, state));
    } else {
      pendingInline.push(...inlineChildren({ childNodes: [el] } as unknown as Node, {}));
    }
  });
  flushInline();
  return out;
}

export async function htmlToDocx(html: string, title = "문서"): Promise<Uint8Array> {
  const doc = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
  const state: BuildState = { orderedInstance: 0 };
  let children = htmlBlocks(doc.body, { quote: false }, state);
  if (children.length === 0) children = [new Paragraph({ children: [] })];

  const document = new Document({
    title,
    creator: "Any LM Desktop",
    styles: {
      default: {
        document: {
          run: { font: { ascii: BODY_FONT, eastAsia: BODY_FONT, hAnsi: BODY_FONT, cs: BODY_FONT }, size: 22 },
          paragraph: { spacing: { after: 120, line: 300 } },
        },
      },
    },
    numbering: {
      config: [
        {
          reference: ORDERED_REF,
          levels: Array.from({ length: 9 }, (_, level) => ({
            level,
            format: LevelFormat.DECIMAL,
            text: `%${level + 1}.`,
            alignment: AlignmentType.START,
            style: { paragraph: { indent: { left: 720 * (level + 1), hanging: 360 } } },
          })),
        },
      ],
    },
    sections: [{ children }],
  });
  return new Uint8Array(await Packer.toArrayBuffer(document));
}

export async function markdownToDocx(md: string, title = "문서"): Promise<Uint8Array> {
  return htmlToDocx(markdownToHtml(md), title);
}
