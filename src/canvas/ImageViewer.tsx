import { Minus, Plus } from "lucide-react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { extensionOf, imageMimeType } from "../docs/formats";
import { imageSize } from "../docs/imageSize";

export interface ImageViewerProps {
  bytes: Uint8Array;
  title: string;
  onError: (msg: string) => void;
}

type BackdropMode = "checker" | "light" | "dark";
type ViewTab = "image" | "code";

const ZOOM_STEPS = [0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 1, 1.25, 1.5, 2, 3, 4, 5];

function formatBytes(len: number): string {
  if (len < 1024) return `${len.toLocaleString()} B`;
  if (len < 1024 * 1024) return `${(len / 1024).toFixed(1)} KB`;
  return `${(len / (1024 * 1024)).toFixed(2)} MB`;
}

function stepZoom(current: number, direction: 1 | -1): number {
  if (direction > 0) {
    const next = ZOOM_STEPS.find((z) => z > current + 0.02);
    return next ?? 5;
  }
  const prev = [...ZOOM_STEPS].reverse().find((z) => z < current - 0.02);
  return prev ?? 0.1;
}

const CHECKER_STYLE: CSSProperties = {
  backgroundColor: "#ffffff",
  backgroundImage:
    "linear-gradient(45deg, #e5e5e0 25%, transparent 25%), " +
    "linear-gradient(-45deg, #e5e5e0 25%, transparent 25%), " +
    "linear-gradient(45deg, transparent 75%, #e5e5e0 75%), " +
    "linear-gradient(-45deg, transparent 75%, #e5e5e0 75%)",
  backgroundSize: "16px 16px",
  backgroundPosition: "0 0, 0 8px, 8px -8px, -8px 0",
};

/** 캔버스에서 이미지 파일을 확대/축소·배경 전환과 함께 미리보기한다 (읽기 전용). */
export function ImageViewer({ bytes, title, onError }: ImageViewerProps) {
  const isSvg = extensionOf(title) === "svg";
  const mime = useMemo(() => imageMimeType(title), [title]);
  const [url, setUrl] = useState<string | null>(null);
  const [dims, setDims] = useState<{ width: number; height: number } | null>(() => imageSize(bytes));
  const [zoom, setZoom] = useState<"fit" | number>("fit");
  const [backdrop, setBackdrop] = useState<BackdropMode>("checker");
  const [view, setView] = useState<ViewTab>("image");
  const [decodeError, setDecodeError] = useState<string | null>(null);

  const viewportRef = useRef<HTMLDivElement | null>(null);
  const [viewportSize, setViewportSize] = useState<{ width: number; height: number }>({ width: 0, height: 0 });

  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;

  useEffect(() => {
    setZoom("fit");
    setView("image");
    setDims(imageSize(bytes));

    if (bytes.byteLength === 0) {
      setUrl(null);
      setDecodeError("빈 이미지 파일입니다.");
      return;
    }

    setDecodeError(null);
    const objectUrl = URL.createObjectURL(new Blob([bytes as BlobPart], { type: mime }));
    setUrl(objectUrl);
    return () => {
      URL.revokeObjectURL(objectUrl);
    };
  }, [bytes, mime]);

  useLayoutEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const update = () => {
      setViewportSize({
        width: Math.max(80, el.clientWidth - 40),
        height: Math.max(80, el.clientHeight - 40),
      });
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, [view]);

  const fitScale = useMemo(() => {
    if (!dims || dims.width <= 0 || dims.height <= 0 || viewportSize.width <= 0 || viewportSize.height <= 0) {
      return 1;
    }
    return Math.min(1, viewportSize.width / dims.width, viewportSize.height / dims.height);
  }, [dims, viewportSize]);

  const effectiveScale = zoom === "fit" ? fitScale : zoom;

  useEffect(() => {
    const el = viewportRef.current;
    if (!el || view !== "image") return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      const dir: 1 | -1 = e.deltaY < 0 ? 1 : -1;
      setZoom((prev) => {
        const base = prev === "fit" ? fitScale : prev;
        return stepZoom(base, dir);
      });
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [view, fitScale]);

  const svgText = useMemo(() => {
    if (!isSvg || view !== "code") return "";
    return new TextDecoder().decode(bytes);
  }, [isSvg, view, bytes]);

  const backdropStyle: CSSProperties =
    backdrop === "checker"
      ? CHECKER_STYLE
      : backdrop === "light"
        ? { backgroundColor: "#ffffff" }
        : { backgroundColor: "#141413" };

  return (
    <div className="flex flex-col h-full min-h-0">
      {/* 상단 정보·확대/축소·배경 툴바 */}
      <div className="flex flex-wrap items-center gap-2 px-3 py-1.5 border-b border-[#E5E3DB] dark:border-[#343430] bg-[#FAF9F5] dark:bg-[#1E1E1C] text-xs text-stone-600 dark:text-stone-300">
        <span className="font-mono text-[11px] tabular-nums">
          {dims && dims.width > 0 && dims.height > 0 ? `${dims.width} × ${dims.height} px · ` : ""}
          {formatBytes(bytes.byteLength)}
        </span>

        <div className="flex-1" />

        {isSvg && (
          <div className="flex bg-stone-200/80 dark:bg-[#2A2A27] p-0.5 rounded-lg">
            {(
              [
                ["image", "이미지"],
                ["code", "코드"],
              ] as const
            ).map(([tab, label]) => (
              <button
                key={tab}
                type="button"
                onClick={() => setView(tab)}
                className={`px-2 py-0.5 rounded-md font-semibold cursor-pointer ${
                  view === tab ? "bg-white dark:bg-[#383834] text-stone-900 dark:text-stone-100 shadow-2xs" : ""
                }`}
              >
                {label}
              </button>
            ))}
          </div>
        )}

        {view === "image" && (
          <>
            <div className="flex bg-stone-200/80 dark:bg-[#2A2A27] p-0.5 rounded-lg" title="투명 배경 표시">
              {(
                [
                  ["checker", "바둑판"],
                  ["light", "밝게"],
                  ["dark", "어둡게"],
                ] as const
              ).map(([mode, label]) => (
                <button
                  key={mode}
                  type="button"
                  onClick={() => setBackdrop(mode)}
                  className={`px-2 py-0.5 rounded-md font-medium cursor-pointer ${
                    backdrop === mode
                      ? "bg-white dark:bg-[#383834] text-stone-900 dark:text-stone-100 shadow-2xs"
                      : "text-stone-500 dark:text-stone-400"
                  }`}
                >
                  {label}
                </button>
              ))}
            </div>

            <div className="flex items-center gap-0.5 border-l border-stone-200 dark:border-[#343430] pl-2">
              <button
                type="button"
                onClick={() => setZoom("fit")}
                className={`px-2 py-0.5 rounded-md font-semibold cursor-pointer ${
                  zoom === "fit"
                    ? "bg-[#A50034]/10 text-[#A50034] dark:bg-[#FF5C8D]/15 dark:text-[#FF5C8D]"
                    : "hover:bg-stone-200 dark:hover:bg-[#32322F]"
                }`}
                title="캔버스 크기에 맞춰 전체 보기"
              >
                맞춤
              </button>
              <button
                type="button"
                onClick={() => setZoom(stepZoom(effectiveScale, -1))}
                className="p-1 rounded hover:bg-stone-200 dark:hover:bg-[#32322F] cursor-pointer"
                title="축소 (Ctrl + 휠)"
                aria-label="이미지 축소"
              >
                <Minus className="w-3.5 h-3.5" />
              </button>
              <button
                type="button"
                onClick={() => setZoom((prev) => (prev === 1 ? "fit" : 1))}
                className="w-12 text-center font-mono cursor-pointer rounded hover:bg-stone-200 dark:hover:bg-[#32322F] py-0.5 tabular-nums"
                title={zoom === 1 ? "화면에 맞추기" : "원본 크기(100%)로 보기"}
              >
                {Math.round(effectiveScale * 100)}%
              </button>
              <button
                type="button"
                onClick={() => setZoom(stepZoom(effectiveScale, 1))}
                className="p-1 rounded hover:bg-stone-200 dark:hover:bg-[#32322F] cursor-pointer"
                title="확대 (Ctrl + 휠)"
                aria-label="이미지 확대"
              >
                <Plus className="w-3.5 h-3.5" />
              </button>
            </div>
          </>
        )}
      </div>

      {/* 본문 영역 */}
      <div ref={viewportRef} className="flex-1 min-h-0 overflow-auto bg-stone-200/70 dark:bg-[#161614] p-5 flex">
        {view === "code" && isSvg ? (
          <pre className="w-full max-w-4xl mx-auto self-start bg-white dark:bg-[#232321] border border-stone-200 dark:border-[#343430] rounded-lg p-5 font-mono text-xs leading-relaxed whitespace-pre-wrap break-all select-text text-stone-800 dark:text-stone-100">
            {svgText}
          </pre>
        ) : decodeError ? (
          <div className="m-auto text-xs text-red-600 dark:text-red-400 bg-white dark:bg-[#232321] border border-red-200 dark:border-red-900/50 rounded-lg px-4 py-3">
            {decodeError}
          </div>
        ) : url ? (
          <div className="m-auto shrink-0 rounded shadow-md overflow-hidden" style={backdropStyle}>
            <img
              src={url}
              alt={title}
              draggable={false}
              onLoad={(e) => {
                const nw = e.currentTarget.naturalWidth;
                const nh = e.currentTarget.naturalHeight;
                if (nw > 0 && nh > 0) setDims({ width: nw, height: nh });
              }}
              onError={() => {
                const msg = "이미지를 표시할 수 없습니다. 손상되었거나 지원하지 않는 형식입니다.";
                setDecodeError(msg);
                onErrorRef.current(msg);
              }}
              style={
                zoom === "fit"
                  ? {
                      display: "block",
                      maxWidth: viewportSize.width > 0 ? viewportSize.width : "100%",
                      maxHeight: viewportSize.height > 0 ? viewportSize.height : "100%",
                      width: "auto",
                      height: "auto",
                    }
                  : dims && dims.width > 0 && dims.height > 0
                    ? {
                        display: "block",
                        width: Math.round(dims.width * zoom),
                        height: Math.round(dims.height * zoom),
                        maxWidth: "none",
                      }
                    : {
                        display: "block",
                        transform: `scale(${zoom})`,
                        transformOrigin: "center",
                      }
              }
            />
          </div>
        ) : null}
      </div>
    </div>
  );
}
