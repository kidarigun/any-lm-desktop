/**
 * 한글 문서(HWPX/HWP)를 원래 모양 그대로 보여 주고, 그 자리에서 글자를 고치는 편집기.
 *
 * 배치·렌더링·편집은 rhwp(@rhwp/core) WASM 엔진이 하고, 여기서는
 *  - 쪽마다 엔진이 그린 SVG를 <img>로 보여 주고 (스크립트가 실행될 수 없다),
 *  - 클릭한 자리를 엔진의 hitTest로 문서 위치로 바꿔 캐럿을 그리고,
 *  - 숨은 textarea로 키보드·한글 입력기(IME) 입력을 받아 엔진에 넣는다.
 * 저장은 엔진이 원래 구조를 보존해 HWPX로 내보낸다.
 */
import type { HwpDocument } from "@rhwp/core";
import { forwardRef, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  caretRect,
  deleteBackward,
  deleteForward,
  insertText,
  lineEdge,
  moveHorizontal,
  moveVertical,
  parseResult,
  posFromRaw,
  splitParagraph,
  type CaretRect,
  type DocPos,
  type RhwpDoc,
} from "./editing";
import { exportHwpx, openRhwpDocument, pageSize, type PageSize } from "./rhwpEngine";

export interface HwpxEditorHandle {
  /** 지금 내용을 HWPX로 내보낸다. lossCount는 엔진이 보존하지 못했다고 알린 요소 수. */
  exportBytes(): { bytes: Uint8Array; lossCount: number };
}

interface HwpxEditorProps {
  bytes: Uint8Array;
  onDirty: () => void;
  onError: (msg: string) => void;
  /** 엔진을 불러오거나 문서를 열지 못했을 때 (캔버스가 단순 편집기로 바꾼다) */
  onLoadFailed: (err: unknown) => void;
}

const PAGE_GAP = 16;
const UNDO_LIMIT = 50;
/** 이만큼 안에 이어서 친 글자는 되돌리기 한 번으로 묶는다 */
const TYPING_GROUP_MS = 1500;

interface UndoEntry {
  snap: number;
  pos: DocPos | null;
}

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

const HwpxEditor = forwardRef<HwpxEditorHandle, HwpxEditorProps>(function HwpxEditor(
  { bytes, onDirty, onError, onLoadFailed },
  ref
) {
  const docRef = useRef<HwpDocument | null>(null);
  const [status, setStatus] = useState<"loading" | "ready">("loading");
  const [pages, setPages] = useState<PageSize[]>([]);
  const [version, setVersion] = useState(0);
  const [images, setImages] = useState<Record<number, string>>({});
  const rendered = useRef(new Map<number, { version: number; url: string }>());
  const pageEls = useRef(new Map<number, HTMLDivElement>());
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const [width, setWidth] = useState(0);

  const caretRef = useRef<DocPos | null>(null);
  const [caret, setCaret] = useState<CaretRect | null>(null);
  const preferredX = useRef(-1);
  const composing = useRef(false);
  const [composeText, setComposeText] = useState("");
  const undoStack = useRef<UndoEntry[]>([]);
  const redoStack = useRef<UndoEntry[]>([]);
  const lastEdit = useRef<{ kind: string; at: number }>({ kind: "", at: 0 });

  const onDirtyRef = useRef(onDirty);
  onDirtyRef.current = onDirty;
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;

  // ---- 문서 열기 ----------------------------------------------------------
  useEffect(() => {
    let cancelled = false;
    setStatus("loading");
    openRhwpDocument(bytes)
      .then((doc) => {
        if (cancelled) {
          doc.free();
          return;
        }
        docRef.current = doc;
        const n = doc.pageCount();
        setPages(Array.from({ length: n }, (_, i) => pageSize(doc, i)));
        setStatus("ready");
      })
      .catch((e) => {
        if (!cancelled) onLoadFailed(e);
      });
    const renderedMap = rendered.current;
    return () => {
      cancelled = true;
      for (const { url } of renderedMap.values()) URL.revokeObjectURL(url);
      renderedMap.clear();
      for (const e of [...undoStack.current, ...redoStack.current]) {
        try {
          docRef.current?.discardSnapshot(e.snap);
        } catch {
          // 문서를 닫으면 함께 사라진다.
        }
      }
      undoStack.current = [];
      redoStack.current = [];
      docRef.current?.free();
      docRef.current = null;
    };
    // bytes가 바뀌면(다른 문서) 새로 연다.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bytes]);

  useImperativeHandle(
    ref,
    () => ({
      exportBytes() {
        const doc = docRef.current;
        if (!doc) throw new Error("한글 문서가 아직 열리지 않았습니다.");
        return exportHwpx(doc);
      },
    }),
    []
  );

  // ---- 배치 --------------------------------------------------------------
  const maxPageWidth = Math.max(1, ...pages.map((p) => p.width));
  const zoom = width > 0 ? Math.min(1.5, Math.max(0.4, (width - 32) / maxPageWidth)) : 1;
  const layout = useMemo(() => {
    let top = 16;
    const offsets = pages.map((p) => {
      const t = top;
      top += p.height * zoom + PAGE_GAP;
      return t;
    });
    return { offsets, total: top };
  }, [pages, zoom]);

  // ---- 쪽 그리기 ----------------------------------------------------------
  /** 화면에 보이는(또는 곧 보일) 쪽만 그린다. 편집 뒤에는 그 쪽들을 다시 그린다. */
  const renderVisible = useCallback(() => {
    const doc = docRef.current;
    const scroller = scrollRef.current;
    if (!doc || !scroller) return;
    const from = scroller.scrollTop - 800;
    const to = scroller.scrollTop + scroller.clientHeight + 800;
    let changed = false;
    const next: Record<number, string> = {};
    for (let i = 0; i < pages.length; i++) {
      const top = layout.offsets[i];
      if (top + pages[i].height * zoom < from || top > to) continue;
      const cur = rendered.current.get(i);
      if (cur && cur.version === version) continue;
      try {
        const svg = doc.renderPageSvg(i);
        const url = URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
        if (cur) URL.revokeObjectURL(cur.url);
        rendered.current.set(i, { version, url });
        next[i] = url;
        changed = true;
      } catch (e) {
        onErrorRef.current(`${i + 1}쪽을 그리지 못했습니다: ${errorText(e)}`);
      }
    }
    if (changed) setImages((prev) => ({ ...prev, ...next }));
  }, [pages, version, layout, zoom]);

  useEffect(() => {
    renderVisible();
  }, [renderVisible, status]);

  const scrollFrame = useRef(0);
  const onScroll = () => {
    if (scrollFrame.current) return;
    scrollFrame.current = requestAnimationFrame(() => {
      scrollFrame.current = 0;
      renderVisible();
    });
  };

  // 쪽 수가 줄면 남은 그림을 정리한다.
  useEffect(() => {
    for (const [i, { url }] of rendered.current) {
      if (i >= pages.length) {
        URL.revokeObjectURL(url);
        rendered.current.delete(i);
      }
    }
  }, [pages.length]);

  // 패널 폭에 맞춰 확대·축소
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, [status]);
  // ---- 캐럿 ---------------------------------------------------------------
  const placeCaret = useCallback((pos: DocPos | null, scroll = true) => {
    caretRef.current = pos;
    const doc = docRef.current;
    const rect = doc && pos ? caretRect(doc as unknown as RhwpDoc, pos) : null;
    setCaret(rect);
    if (rect && scroll) {
      requestAnimationFrame(() => {
        const pageEl = pageEls.current.get(rect.page);
        const scroller = scrollRef.current;
        if (!pageEl || !scroller) return;
        const top = pageEl.offsetTop + rect.y * zoomRef.current;
        const bottom = top + rect.height * zoomRef.current;
        if (top < scroller.scrollTop + 8) scroller.scrollTop = top - 40;
        else if (bottom > scroller.scrollTop + scroller.clientHeight - 8) scroller.scrollTop = bottom - scroller.clientHeight + 40;
      });
    }
  }, []);
  const zoomRef = useRef(zoom);
  zoomRef.current = zoom;

  /** 편집 뒤: 쪽 수·크기를 다시 읽고, 바뀐 쪽을 다시 그린다. */
  const afterEdit = useCallback(
    (pos: DocPos | null) => {
      const doc = docRef.current;
      if (!doc) return;
      const n = doc.pageCount();
      setPages((prev) => {
        if (prev.length === n) return prev;
        return Array.from({ length: n }, (_, i) => prev[i] ?? pageSize(doc, i));
      });
      setVersion((v) => v + 1);
      placeCaret(pos);
      onDirtyRef.current();
    },
    [placeCaret]
  );

  // ---- 되돌리기 -----------------------------------------------------------
  const dropEntries = (list: UndoEntry[]) => {
    for (const e of list) {
      try {
        docRef.current?.discardSnapshot(e.snap);
      } catch {
        // 무시
      }
    }
  };

  const remember = (kind: string) => {
    const doc = docRef.current;
    if (!doc) return;
    const now = Date.now();
    const last = lastEdit.current;
    lastEdit.current = { kind, at: now };
    if (kind === "type" && last.kind === "type" && now - last.at < TYPING_GROUP_MS) return;
    undoStack.current.push({ snap: doc.saveSnapshot(), pos: caretRef.current });
    if (undoStack.current.length > UNDO_LIMIT) dropEntries(undoStack.current.splice(0, 1));
    dropEntries(redoStack.current);
    redoStack.current = [];
  };

  const restore = (from: UndoEntry[], to: UndoEntry[]) => {
    const doc = docRef.current;
    const entry = from.pop();
    if (!doc || !entry) return;
    to.push({ snap: doc.saveSnapshot(), pos: caretRef.current });
    try {
      parseResult(doc.restoreSnapshot(entry.snap));
    } catch (e) {
      onErrorRef.current(`되돌리지 못했습니다: ${errorText(e)}`);
    }
    try {
      doc.discardSnapshot(entry.snap);
    } catch {
      // 무시
    }
    lastEdit.current = { kind: "", at: 0 };
    afterEdit(entry.pos);
  };

  // ---- 편집 ---------------------------------------------------------------
  const edit = (kind: string, fn: (doc: RhwpDoc, pos: DocPos) => DocPos | null) => {
    const doc = docRef.current;
    const pos = caretRef.current;
    if (!doc || !pos) return;
    remember(kind);
    try {
      const next = fn(doc as unknown as RhwpDoc, pos);
      preferredX.current = -1;
      if (next) afterEdit(next);
      else undoStack.current.pop();
    } catch (e) {
      onErrorRef.current(`고치지 못했습니다: ${errorText(e)}`);
      afterEdit(pos);
    }
  };

  const move = (fn: (doc: RhwpDoc, pos: DocPos) => DocPos, keepX = false) => {
    const doc = docRef.current;
    const pos = caretRef.current;
    if (!doc || !pos) return;
    try {
      if (!keepX) preferredX.current = -1;
      lastEdit.current = { kind: "", at: 0 };
      placeCaret(fn(doc as unknown as RhwpDoc, pos));
    } catch {
      // 움직일 수 없는 자리면 그대로 둔다.
    }
  };

  const typeText = (text: string) => {
    if (text) edit("type", (doc, pos) => insertText(doc, pos, text));
  };

  const onPageMouseDown = (page: number, e: React.MouseEvent<HTMLDivElement>) => {
    const doc = docRef.current;
    if (!doc || e.button !== 0) return;
    e.preventDefault();
    const box = e.currentTarget.getBoundingClientRect();
    const x = (e.clientX - box.left) / zoom;
    const y = (e.clientY - box.top) / zoom;
    try {
      const pos = posFromRaw(parseResult(doc.hitTest(page, x, y)));
      preferredX.current = -1;
      lastEdit.current = { kind: "", at: 0 };
      placeCaret(pos, false);
    } catch {
      placeCaret(null, false);
    }
    inputRef.current?.focus({ preventScroll: true });
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (composing.current || e.nativeEvent.isComposing || e.keyCode === 229) return;
    const ctrl = e.ctrlKey || e.metaKey;
    const key = e.key;
    if (ctrl && (key === "z" || key === "Z") && !e.shiftKey) {
      e.preventDefault();
      restore(undoStack.current, redoStack.current);
      return;
    }
    if (ctrl && (key === "y" || ((key === "z" || key === "Z") && e.shiftKey))) {
      e.preventDefault();
      restore(redoStack.current, undoStack.current);
      return;
    }
    if (ctrl) return; // Ctrl+S 등은 캔버스가 처리한다.
    switch (key) {
      case "Backspace":
        e.preventDefault();
        edit("delete", deleteBackward);
        break;
      case "Delete":
        e.preventDefault();
        edit("delete", deleteForward);
        break;
      case "Enter":
        e.preventDefault();
        edit("split", splitParagraph);
        break;
      case "ArrowLeft":
      case "ArrowRight":
        e.preventDefault();
        move((doc, pos) => moveHorizontal(doc, pos, key === "ArrowLeft" ? -1 : 1));
        break;
      case "ArrowUp":
      case "ArrowDown":
        e.preventDefault();
        move((doc, pos) => {
          const r = moveVertical(doc, pos, key === "ArrowUp" ? -1 : 1, preferredX.current);
          preferredX.current = r.preferredX;
          return r.pos;
        }, true);
        break;
      case "Home":
      case "End":
        e.preventDefault();
        move((doc, pos) => lineEdge(doc, pos, key === "Home" ? "start" : "end"));
        break;
      case "Tab":
        e.preventDefault();
        typeText("\t");
        break;
    }
  };

  const onInput = (e: React.FormEvent<HTMLTextAreaElement>) => {
    if (composing.current) return;
    const ta = e.currentTarget;
    const text = ta.value;
    ta.value = "";
    typeText(text);
  };

  const onPaste = (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    e.preventDefault();
    typeText(e.clipboardData.getData("text/plain"));
  };

  // ---- 화면 ---------------------------------------------------------------
  if (status === "loading") {
    return <div className="p-6 text-xs text-stone-500">한글 문서 엔진을 불러오는 중...</div>;
  }

  const { offsets, total } = layout;

  const caretBox =
    caret && caret.page < pages.length
      ? {
          left: ((maxPageWidth - pages[caret.page].width) * zoom) / 2 + caret.x * zoom,
          top: offsets[caret.page] + caret.y * zoom,
          height: Math.max(8, caret.height * zoom),
        }
      : null;

  return (
    <div ref={scrollRef} onScroll={onScroll} className="h-full overflow-auto bg-[#E7E5DF] dark:bg-[#141413]">
      <div className="relative mx-auto" style={{ width: maxPageWidth * zoom, height: total + 16 }}>
        {pages.map((p, i) => (
          <div
            key={i}
            data-page={i}
            ref={(el) => {
              if (el) pageEls.current.set(i, el);
              else pageEls.current.delete(i);
            }}
            onMouseDown={(e) => onPageMouseDown(i, e)}
            className="absolute bg-white shadow-sm cursor-text select-none"
            style={{ top: offsets[i], left: ((maxPageWidth - p.width) * zoom) / 2, width: p.width * zoom, height: p.height * zoom }}
          >
            {images[i] && (
              <img src={images[i]} alt={`${i + 1}쪽`} draggable={false} className="block w-full h-full pointer-events-none" />
            )}
          </div>
        ))}
        {caretBox && (
          <>
            <div
              className="absolute w-[2px] bg-[#A50034] hwpx-caret pointer-events-none"
              style={{ left: caretBox.left - 1, top: caretBox.top, height: caretBox.height }}
            />
            {composeText && (
              <span
                className="absolute whitespace-pre bg-white text-black border-b-2 border-[#A50034] pointer-events-none"
                style={{ left: caretBox.left, top: caretBox.top, fontSize: Math.max(10, caretBox.height * 0.8), lineHeight: `${caretBox.height}px` }}
              >
                {composeText}
              </span>
            )}
          </>
        )}
        <textarea
          ref={inputRef}
          aria-label="한글 문서 입력"
          className="absolute opacity-0 w-px h-px p-0 border-0 resize-none overflow-hidden"
          style={{ left: caretBox?.left ?? 0, top: caretBox?.top ?? 16 }}
          autoComplete="off"
          autoCorrect="off"
          spellCheck={false}
          onKeyDown={onKeyDown}
          onInput={onInput}
          onPaste={onPaste}
          onCompositionStart={() => {
            composing.current = true;
          }}
          onCompositionUpdate={(e) => setComposeText(e.data)}
          onCompositionEnd={(e) => {
            composing.current = false;
            setComposeText("");
            e.currentTarget.value = "";
            typeText(e.data);
          }}
          onBlur={() => {
            composing.current = false;
            setComposeText("");
          }}
        />
      </div>
    </div>
  );
});

export default HwpxEditor;
