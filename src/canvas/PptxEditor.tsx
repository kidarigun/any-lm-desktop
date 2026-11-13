import { ChevronLeft, ChevronRight, ImageOff, StickyNote } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import type { PptxDeck, PptxParagraph, PptxRun, PptxShape, PptxSlide } from "../docs/pptx";

const EMU_PER_PT = 12700;

export interface PptxEditorProps {
  deck: PptxDeck;
  edits: Map<string, string>;
  onEdit: (key: string, text: string) => void;
}

/** 슬라이드를 원래 배치대로 그리고, 텍스트 상자를 직접 클릭해 고칠 수 있게 한다. */
export function PptxEditor({ deck, edits, onEdit }: PptxEditorProps) {
  const [current, setCurrent] = useState(0);
  const mainRef = useRef<HTMLDivElement | null>(null);
  const [mainWidth, setMainWidth] = useState(640);

  useLayoutEffect(() => {
    const el = mainRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setMainWidth(Math.max(240, el.clientWidth - 40)));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    if (current >= deck.slides.length) setCurrent(Math.max(0, deck.slides.length - 1));
  }, [deck, current]);

  const slide = deck.slides[current];
  if (!slide) {
    return <div className="p-6 text-xs text-stone-500">슬라이드가 없습니다.</div>;
  }

  return (
    <div className="flex h-full min-h-0">
      <div className="w-32 shrink-0 overflow-y-auto border-r border-[#E5E3DB] dark:border-[#343430] bg-[#F6F5F0] dark:bg-[#1B1B19] p-2 space-y-2">
        {deck.slides.map((s, i) => (
          <button
            key={s.path}
            onClick={() => setCurrent(i)}
            className={`block w-full text-left rounded-md p-1 cursor-pointer ${
              i === current ? "ring-2 ring-[#A50034] dark:ring-[#FF5C8D]" : "hover:bg-stone-200 dark:hover:bg-[#2C2C29]"
            }`}
          >
            <div className="text-[10px] text-stone-500 dark:text-stone-400 mb-0.5">{i + 1}</div>
            <SlideView deck={deck} slide={s} width={108} edits={edits} editable={false} />
          </button>
        ))}
      </div>

      <div ref={mainRef} className="flex-1 min-w-0 overflow-y-auto bg-stone-200/70 dark:bg-[#161614] p-5">
        <div className="flex items-center justify-between mb-3 text-xs text-stone-600 dark:text-stone-300">
          <button
            onClick={() => setCurrent((c) => Math.max(0, c - 1))}
            disabled={current === 0}
            className="p-1 rounded-md hover:bg-stone-300 dark:hover:bg-[#2C2C29] disabled:opacity-30 cursor-pointer"
            title="이전 슬라이드"
          >
            <ChevronLeft className="w-4 h-4" />
          </button>
          <span className="font-semibold">
            슬라이드 {current + 1} / {deck.slides.length}
          </span>
          <button
            onClick={() => setCurrent((c) => Math.min(deck.slides.length - 1, c + 1))}
            disabled={current === deck.slides.length - 1}
            className="p-1 rounded-md hover:bg-stone-300 dark:hover:bg-[#2C2C29] disabled:opacity-30 cursor-pointer"
            title="다음 슬라이드"
          >
            <ChevronRight className="w-4 h-4" />
          </button>
        </div>

        <div className="mx-auto shadow-lg" style={{ width: mainWidth }}>
          <SlideView key={slide.path} deck={deck} slide={slide} width={mainWidth} edits={edits} editable onEdit={onEdit} />
        </div>

        {slide.notes && (
          <div className="mx-auto mt-3 rounded-lg bg-white/80 dark:bg-[#232321] border border-stone-300 dark:border-[#3A3A36] p-3 text-xs text-stone-600 dark:text-stone-300 whitespace-pre-wrap" style={{ width: mainWidth }}>
            <div className="flex items-center gap-1 font-semibold mb-1">
              <StickyNote className="w-3.5 h-3.5" /> 발표자 노트
            </div>
            {slide.notes}
          </div>
        )}
      </div>
    </div>
  );
}

function SlideView({
  deck,
  slide,
  width,
  edits,
  editable,
  onEdit,
}: {
  deck: PptxDeck;
  slide: PptxSlide;
  width: number;
  edits: Map<string, string>;
  editable: boolean;
  onEdit?: (key: string, text: string) => void;
}) {
  const scale = width / deck.width;
  return (
    <div
      className="relative overflow-hidden select-text"
      style={{ width, height: deck.height * scale, background: slide.background }}
    >
      {slide.shapes.map((shape) => (
        <ShapeView key={shape.key} shape={shape} scale={scale} edits={edits} editable={editable} onEdit={onEdit} />
      ))}
    </div>
  );
}

function ShapeView({
  shape,
  scale,
  edits,
  editable,
  onEdit,
}: {
  shape: PptxShape;
  scale: number;
  edits: Map<string, string>;
  editable: boolean;
  onEdit?: (key: string, text: string) => void;
}) {
  const style: CSSProperties = {
    position: "absolute",
    left: shape.x * scale,
    top: shape.y * scale,
    width: shape.w * scale,
    height: shape.h * scale,
    transform: shape.rot ? `rotate(${shape.rot}deg)` : undefined,
    background: shape.fill,
    border: shape.line ? `1px solid ${shape.line}` : undefined,
    borderRadius: shape.geom === "ellipse" ? "50%" : shape.geom === "roundRect" ? Math.min(shape.w, shape.h) * scale * 0.15 : undefined,
  };

  if (shape.type === "image") {
    return shape.imageSrc ? (
      <img src={shape.imageSrc} style={{ ...style, objectFit: "fill" }} alt="" draggable={false} />
    ) : (
      <div style={style} className="flex items-center justify-center bg-stone-100/70 text-stone-400">
        <ImageOff style={{ width: 14, height: 14 }} />
      </div>
    );
  }

  if (shape.type === "table" && shape.table) {
    const fontPx = Math.max(4, 12 * EMU_PER_PT * scale);
    const total = shape.table.colWidths.reduce((a, b) => a + b, 0) || 1;
    return (
      <div style={style}>
        <table className="w-full h-full border-collapse" style={{ fontSize: fontPx, tableLayout: "fixed" }}>
          <colgroup>
            {shape.table.colWidths.map((w, i) => (
              <col key={i} style={{ width: `${(w / total) * 100}%` }} />
            ))}
          </colgroup>
          <tbody>
            {shape.table.rows.map((row, ri) => (
              <tr key={ri}>
                {row.map((cell, ci) => (
                  <td
                    key={ci}
                    className="border border-stone-400/70 align-top whitespace-pre-wrap"
                    style={{ padding: fontPx * 0.3, fontWeight: ri === 0 ? 700 : 400, background: ri === 0 ? "#F2F2F2" : undefined, color: "#222" }}
                  >
                    {cell}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }

  if (shape.type !== "text") return <div style={style} />;

  const [l, t, r, b] = shape.insets;
  return (
    <div
      style={{
        ...style,
        display: "flex",
        flexDirection: "column",
        justifyContent: shape.anchor === "middle" ? "center" : shape.anchor === "bottom" ? "flex-end" : "flex-start",
        padding: `${t * scale}px ${r * scale}px ${b * scale}px ${l * scale}px`,
        overflow: "visible",
      }}
    >
      {shape.paragraphs.map((p) => (
        <ParagraphView key={p.key} p={p} scale={scale} value={edits.get(p.key)} editable={editable} onEdit={onEdit} />
      ))}
    </div>
  );
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}

function fontFamily(run?: PptxRun): string {
  const f = run?.font && !run.font.startsWith("+") ? `"${run.font}", ` : "";
  return `${f}"Malgun Gothic", "Apple SD Gothic Neo", sans-serif`;
}

function runsHtml(runs: PptxRun[], scale: number): string {
  return runs
    .map((r) => {
      if (r.text === "\n") return "<br>";
      const css = [
        `font-size:${Math.max(3, r.sizePt * EMU_PER_PT * scale)}px`,
        r.bold ? "font-weight:700" : "",
        r.italic ? "font-style:italic" : "",
        r.underline ? "text-decoration:underline" : "",
        r.color ? `color:${r.color}` : "",
        `font-family:${fontFamily(r).replace(/"/g, "'")}`,
      ]
        .filter(Boolean)
        .join(";");
      return `<span style="${css}">${escapeHtml(r.text)}</span>`;
    })
    .join("");
}

function ParagraphView({
  p,
  scale,
  value,
  editable,
  onEdit,
}: {
  p: PptxParagraph;
  scale: number;
  value: string | undefined;
  editable: boolean;
  onEdit?: (key: string, text: string) => void;
}) {
  const ref = useRef<HTMLDivElement | null>(null);
  editable = editable && !p.readOnly;
  const first = p.runs.find((r) => r.text !== "\n");
  const sizePx = Math.max(3, (first?.sizePt ?? p.sizePt) * EMU_PER_PT * scale);
  const color = first?.color ?? "#1F1E1D";

  // 편집 중 React가 내용을 덮어쓰지 않도록 처음 한 번만 내용을 채운다.
  useLayoutEffect(() => {
    if (!ref.current) return;
    ref.current.innerHTML =
      value !== undefined ? escapeHtml(value).replace(/\n/g, "<br>") : runsHtml(p.runs, scale) || "";
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [p.key, scale]);

  const align = p.align === "center" ? "center" : p.align === "right" ? "right" : p.align === "justify" ? "justify" : "left";
  return (
    <div
      style={{
        display: "flex",
        gap: sizePx * 0.4,
        paddingLeft: p.level * sizePx * 1.4,
        lineHeight: 1.2,
        minHeight: sizePx * 1.2,
        textAlign: align,
        color,
        fontFamily: fontFamily(first),
        fontSize: sizePx,
      }}
    >
      {p.bullet && <span style={{ flexShrink: 0 }}>{p.bullet}</span>}
      <div
        ref={ref}
        contentEditable={editable ? "plaintext-only" : false}
        suppressContentEditableWarning
        spellCheck={false}
        onInput={(e) => onEdit?.(p.key, (e.currentTarget as HTMLDivElement).innerText.replace(/\n$/, ""))}
        className={`flex-1 min-w-0 whitespace-pre-wrap break-words outline-none ${
          editable ? "rounded-sm hover:bg-[#A50034]/5 focus:bg-[#A50034]/5 focus:ring-1 focus:ring-[#A50034]/40 cursor-text" : ""
        }`}
        style={{ textAlign: align }}
      />
    </div>
  );
}
