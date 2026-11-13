import { FileText, Minus, Plus } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { extractPdfText, openPdf, type OpenedPdf } from "../docs/pdf";

/** PDF를 페이지별로 그려 보여 준다. "텍스트" 모드에서는 추출한 텍스트를 선택·복사할 수 있다. */
export function PdfViewer({ bytes, onError }: { bytes: Uint8Array; onError: (msg: string) => void }) {
  const [pdf, setPdf] = useState<OpenedPdf | null>(null);
  const [zoom, setZoom] = useState(1);
  const [mode, setMode] = useState<"pages" | "text">("pages");
  const [text, setText] = useState<string | null>(null);
  const boxRef = useRef<HTMLDivElement | null>(null);
  const [boxWidth, setBoxWidth] = useState(600);

  useEffect(() => {
    let cancelled = false;
    let opened: OpenedPdf | null = null;
    setPdf(null);
    setText(null);
    openPdf(bytes)
      .then((p) => {
        opened = p;
        if (cancelled) p.close();
        else setPdf(p);
      })
      .catch((e) => onError(`PDF를 열 수 없습니다: ${e?.message || e}`));
    return () => {
      cancelled = true;
      opened?.close();
    };
  }, [bytes, onError]);

  useLayoutEffect(() => {
    const el = boxRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setBoxWidth(Math.max(200, el.clientWidth - 40)));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    if (mode !== "text" || text !== null) return;
    extractPdfText(bytes)
      .then(setText)
      .catch((e) => setText(`텍스트를 추출하지 못했습니다: ${e?.message || e}`));
  }, [mode, text, bytes]);

  return (
    <div className="flex flex-col h-full min-h-0">
      <div className="flex items-center gap-2 px-3 py-1.5 border-b border-[#E5E3DB] dark:border-[#343430] bg-[#FAF9F5] dark:bg-[#1E1E1C] text-xs text-stone-600 dark:text-stone-300">
        <span>{pdf ? `${pdf.doc.numPages}쪽` : "불러오는 중..."}</span>
        <div className="flex-1" />
        <div className="flex bg-stone-200/80 dark:bg-[#2A2A27] p-0.5 rounded-lg">
          {(["pages", "text"] as const).map((m) => (
            <button
              key={m}
              onClick={() => setMode(m)}
              className={`px-2 py-0.5 rounded-md font-semibold cursor-pointer ${
                mode === m ? "bg-white dark:bg-[#383834] text-stone-900 dark:text-stone-100 shadow-2xs" : ""
              }`}
            >
              {m === "pages" ? "페이지" : "텍스트"}
            </button>
          ))}
        </div>
        {mode === "pages" && (
          <>
            <button onClick={() => setZoom((z) => Math.max(0.4, +(z - 0.1).toFixed(2)))} className="p-1 rounded hover:bg-stone-200 dark:hover:bg-[#32322F] cursor-pointer" title="축소">
              <Minus className="w-3.5 h-3.5" />
            </button>
            <button onClick={() => setZoom(1)} className="w-11 text-center font-mono cursor-pointer" title="폭에 맞추기">
              {Math.round(zoom * 100)}%
            </button>
            <button onClick={() => setZoom((z) => Math.min(3, +(z + 0.1).toFixed(2)))} className="p-1 rounded hover:bg-stone-200 dark:hover:bg-[#32322F] cursor-pointer" title="확대">
              <Plus className="w-3.5 h-3.5" />
            </button>
          </>
        )}
      </div>

      <div ref={boxRef} className="flex-1 overflow-auto bg-stone-200/70 dark:bg-[#161614] p-5">
        {mode === "text" ? (
          <div className="mx-auto max-w-3xl bg-white dark:bg-[#232321] rounded-lg p-5 text-xs leading-relaxed whitespace-pre-wrap select-text text-stone-800 dark:text-stone-100">
            {text === null ? (
              <span className="flex items-center gap-1.5 text-stone-500">
                <FileText className="w-3.5 h-3.5" /> 텍스트를 추출하고 있습니다...
              </span>
            ) : (
              text
            )}
          </div>
        ) : (
          pdf && (
            <div className="space-y-4">
              {Array.from({ length: pdf.doc.numPages }, (_, i) => (
                <PdfPage key={i} pdf={pdf} pageNumber={i + 1} width={boxWidth * zoom} />
              ))}
            </div>
          )
        )}
      </div>
    </div>
  );
}

function PdfPage({ pdf, pageNumber, width }: { pdf: OpenedPdf; pageNumber: number; width: number }) {
  const holderRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [visible, setVisible] = useState(pageNumber <= 2);
  const [aspect, setAspect] = useState(1.414);

  // 화면 근처에 온 페이지만 그린다.
  useEffect(() => {
    const el = holderRef.current;
    if (!el || visible) return;
    const io = new IntersectionObserver((entries) => entries.some((e) => e.isIntersecting) && setVisible(true), {
      rootMargin: "600px",
    });
    io.observe(el);
    return () => io.disconnect();
  }, [visible]);

  useEffect(() => {
    if (!visible) return;
    let cancelled = false;
    let task: { cancel: () => void; promise: Promise<void> } | null = null;
    (async () => {
      const page = await pdf.doc.getPage(pageNumber);
      if (cancelled) return;
      const base = page.getViewport({ scale: 1 });
      setAspect(base.height / base.width);
      const dpr = window.devicePixelRatio || 1;
      const viewport = page.getViewport({ scale: (width / base.width) * dpr });
      const canvas = canvasRef.current;
      if (!canvas) return;
      canvas.width = Math.floor(viewport.width);
      canvas.height = Math.floor(viewport.height);
      task = page.render({ canvas, viewport });
      try {
        await task.promise;
      } catch {
        // 크기 변경 등으로 취소된 렌더링
      }
    })();
    return () => {
      cancelled = true;
      task?.cancel();
    };
  }, [pdf, pageNumber, width, visible]);

  return (
    <div ref={holderRef} className="mx-auto bg-white shadow-md" style={{ width, height: width * aspect }}>
      <canvas ref={canvasRef} style={{ width, height: width * aspect, display: "block" }} />
    </div>
  );
}
