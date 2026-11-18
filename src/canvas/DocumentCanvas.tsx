import {
  Check,
  ChevronDown,
  Copy,
  ExternalLink,
  History,
  Info,
  Maximize2,
  Minimize2,
  Paperclip,
  Play,
  Save,
  SaveAll,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { BackendAPI } from "../api";
import { bytesToBase64 } from "../docs/base64";
import { docxToHtml, htmlToDocx } from "../docs/docx";
import { htmlToHwpx, hwpxToHtml, hwpxToMarkdown } from "../docs/hwpx";
import {
  KIND_EXTENSION,
  KIND_LABEL,
  SAVE_AS_TARGETS,
  extensionOf,
  isPreviewOnlyKind,
  replaceExtension,
  type DocKind,
} from "../docs/formats";
import { extractBodyHtml, htmlToMarkdown, markdownToHtml, wrapHtmlDocument } from "../docs/markdown";
import { extractPdfText } from "../docs/pdf";
import { deckToMarkdown, markdownToPptx, parsePptx, type PptxSession } from "../docs/pptx";
import { markdownToXlsx, xlsxToMarkdown } from "../docs/xlsx";
import type { FileSnapshot } from "../types";
import type { CanvasDoc, CanvasSavedInfo } from "./canvasDoc";
import { HtmlPreview } from "./HtmlWebPreview";
import { ImageViewer } from "./ImageViewer";
import { PdfViewer } from "./PdfViewer";
import { PptxViewer } from "./PptxViewer";
import HwpxEditor, { type HwpxEditorHandle } from "./hwpx/HwpxEditor";
import { RichTextEditor } from "./RichTextEditor";
import { XlsxViewer } from "./XlsxViewer";

export interface DocumentCanvasProps {
  doc: CanvasDoc;
  projectId: string | null;
  projectRoot?: string;
  expanded: boolean;
  onToggleExpand: () => void;
  onClose: () => void;
  onSaved: (info: CanvasSavedInfo) => void;
  onAttach?: (absolutePath: string) => void;
  onError: (msg: string) => void;
  /** 이 파일의 변경 기록 수 (AI 도구·직접 저장 모두) */
  historyCount?: number;
  onShowHistory?: () => void;
  /** .py 파일일 때: Python 실행 창 열기 */
  onRunPython?: () => void;
  /** 미리보기 전용 문서(Excel·PowerPoint 등)를 Windows 기본 앱으로 열기 */
  onOpenExternal?: () => void;
}

type Mode = "rich" | "source" | "preview";

function joinPath(root: string, rel: string): string {
  return `${root.replace(/[\\/]+$/, "")}\\${rel.replace(/\//g, "\\")}`;
}

function dirOf(p: string): string {
  const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return i > 0 ? p.slice(0, i) : "";
}

export default function DocumentCanvas({
  doc,
  projectId,
  projectRoot,
  expanded,
  onToggleExpand,
  onClose,
  onSaved,
  onAttach,
  onError,
  historyCount = 0,
  onShowHistory,
  onRunPython,
  onOpenExternal,
}: DocumentCanvasProps) {
  /** PowerPoint·Excel·PDF는 미리보기만 한다 (저장·편집 없음). */
  const previewOnly = isPreviewOnlyKind(doc.kind);
  /** 원래 앱에서 열어 고치는 단추를 보일 문서 (한글 문서는 캔버스에서도 고칠 수 있지만 서식이 단순해진다) */
  const externalApp =
    doc.kind === "xlsx" ? "Excel" : doc.kind === "pptx" ? "PowerPoint" : doc.kind === "hwpx" ? "한글" : previewOnly ? "기본 앱" : null;
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [mode, setMode] = useState<Mode>("rich");
  const [editorKey, setEditorKey] = useState(0);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [savedFlash, setSavedFlash] = useState(false);
  const [saveAsOpen, setSaveAsOpen] = useState(false);
  const [confirmClose, setConfirmClose] = useState(false);
  const [copied, setCopied] = useState(false);
  const [noticeHidden, setNoticeHidden] = useState(false);

  // 현재 편집 내용 (렌더링과 무관하게 최신 값을 유지)
  const htmlRef = useRef("");
  const textRef = useRef("");
  const [initialHtml, setInitialHtml] = useState("");
  const [sourceText, setSourceText] = useState("");
  const [pptx, setPptx] = useState<PptxSession | null>(null);
  /** 한글 문서: "engine"은 원본 모양 편집기(rhwp), "simple"은 엔진을 쓸 수 없을 때의 단순 편집기 */
  const [hwpxMode, setHwpxMode] = useState<"engine" | "simple">("engine");
  const hwpxRef = useRef<HwpxEditorHandle>(null);

  // 문서 불러오기
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setLoadError(null);
    setDirty(false);
    setConfirmClose(false);
    setNoticeHidden(false);
    setPptx(null);
    (async () => {
      try {
        switch (doc.kind) {
          case "markdown": {
            const text = doc.text ?? "";
            textRef.current = text;
            htmlRef.current = markdownToHtml(text);
            setSourceText(text);
            setInitialHtml(htmlRef.current);
            setMode("rich");
            break;
          }
          case "docx": {
            const html = await docxToHtml(doc.bytes ?? new Uint8Array());
            htmlRef.current = html;
            setInitialHtml(html);
            setMode("rich");
            break;
          }
          case "hwpx":
            // 원본 모양 편집기가 스스로 문서를 연다. 실패하면 단순 편집기로 바꾼다(fallbackToSimpleHwpx).
            setHwpxMode("engine");
            setMode("rich");
            break;
          case "pptx": {
            const session = await parsePptx(doc.bytes ?? new Uint8Array());
            if (!cancelled) setPptx(session);
            break;
          }
          case "html": {
            textRef.current = doc.text ?? "";
            setSourceText(textRef.current);
            setMode("preview");
            break;
          }
          case "text": {
            textRef.current = doc.text ?? "";
            setSourceText(textRef.current);
            setMode("source");
            break;
          }
          case "xlsx":
          case "pdf":
          case "image":
            break;
        }
        if (!cancelled) setEditorKey((k) => k + 1);
      } catch (e) {
        if (!cancelled) setLoadError(`문서를 열 수 없습니다: ${e instanceof Error ? e.message : String(e)}`);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
    // 저장·이름 바꾸기로 경로나 제목만 바뀐 경우에는 편집 중인 내용을 다시 불러오지 않는다.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc.id]);

  const markDirty = useCallback(() => setDirty(true), []);

  /** 한글 문서 엔진을 쓸 수 없으면 단순 편집기(서식 단순화)로 연다. */
  const fallbackToSimpleHwpx = useCallback(
    async (err: unknown) => {
      console.warn("rhwp 엔진으로 열지 못해 단순 편집기로 엽니다", err);
      try {
        const html = await hwpxToHtml(doc.bytes ?? new Uint8Array());
        htmlRef.current = html;
        setInitialHtml(html);
        setHwpxMode("simple");
        setEditorKey((k) => k + 1);
      } catch (e) {
        setLoadError(`문서를 열 수 없습니다: ${e instanceof Error ? e.message : String(e)}`);
      }
    },
    [doc.bytes]
  );

  /** 원본 모양 편집기의 현재 내용 (HWPX 바이트). 엔진 편집기가 아니면 null. */
  function engineHwpx(): Uint8Array | null {
    if (doc.kind !== "hwpx" || hwpxMode !== "engine" || !hwpxRef.current) return null;
    const { bytes, lossCount } = hwpxRef.current.exportBytes();
    if (lossCount > 0) {
      onError(`한글 문서 엔진이 저장하면서 ${lossCount}개 요소를 그대로 보존하지 못했다고 알렸습니다. 문제가 있으면 [변경 기록]에서 되돌리세요.`);
    }
    return bytes;
  }

  // Markdown 서식 편집 ↔ 원문 편집 전환
  function switchMode(next: Mode) {
    if (next === mode) return;
    if (doc.kind === "markdown") {
      if (next === "source") {
        textRef.current = htmlToMarkdown(htmlRef.current);
        setSourceText(textRef.current);
      } else {
        htmlRef.current = markdownToHtml(textRef.current);
        setInitialHtml(htmlRef.current);
        setEditorKey((k) => k + 1);
      }
    }
    setMode(next);
  }

  // ---- 내보내기 ----------------------------------------------------------

  async function currentMarkdown(): Promise<string> {
    switch (doc.kind) {
      case "markdown":
        return mode === "rich" ? htmlToMarkdown(htmlRef.current) : textRef.current;
      case "docx":
        return htmlToMarkdown(htmlRef.current);
      case "hwpx": {
        const bytes = engineHwpx();
        return bytes ? hwpxToMarkdown(bytes) : htmlToMarkdown(htmlRef.current);
      }
      case "html":
        return htmlToMarkdown(extractBodyHtml(textRef.current));
      case "pptx":
        return pptx ? deckToMarkdown(pptx.deck) : "";
      case "xlsx":
        return xlsxToMarkdown(doc.bytes ?? new Uint8Array());
      case "pdf":
        return extractPdfText(doc.bytes ?? new Uint8Array());
      default:
        return textRef.current;
    }
  }

  async function currentHtml(): Promise<string> {
    switch (doc.kind) {
      case "markdown":
        return mode === "rich" ? htmlRef.current : markdownToHtml(textRef.current);
      case "docx":
        return htmlRef.current;
      case "hwpx": {
        const bytes = engineHwpx();
        return bytes ? hwpxToHtml(bytes) : htmlRef.current;
      }
      case "html":
        return extractBodyHtml(textRef.current);
      default:
        return markdownToHtml(textRef.current);
    }
  }

  const baseTitle = useMemo(() => doc.title.replace(/\.[^.]+$/, "") || "문서", [doc.title]);

  async function exportAs(target: DocKind): Promise<Uint8Array | string> {
    switch (target) {
      case "markdown":
        return currentMarkdown();
      case "text":
        return doc.kind === "text" ? textRef.current : currentMarkdown();
      case "html":
        return doc.kind === "html" ? textRef.current : wrapHtmlDocument(await currentHtml(), baseTitle);
      case "docx":
        return htmlToDocx(await currentHtml(), baseTitle);
      case "hwpx": {
        // 원본 모양 편집기는 원래 구조를 그대로 내보낸다.
        const bytes = engineHwpx();
        if (bytes) return bytes;
        // 단순 편집기로 다시 저장할 때는 원래 용지 크기·여백을 유지한다.
        return htmlToHwpx(await currentHtml(), baseTitle, doc.kind === "hwpx" ? { base: doc.bytes } : {});
      }
      case "pptx":
        if (doc.kind === "pptx") return (doc.bytes ?? new Uint8Array()).slice();
        return markdownToPptx(await currentMarkdown(), baseTitle);
      case "xlsx":
        if (doc.kind === "xlsx") return (doc.bytes ?? new Uint8Array()).slice();
        return markdownToXlsx(await currentMarkdown(), baseTitle);
      case "pdf":
        return (doc.bytes ?? new Uint8Array()).slice();
      case "image":
        return (doc.bytes ?? new Uint8Array()).slice();
    }
  }

  function toBase64(data: Uint8Array | string): string {
    return typeof data === "string" ? bytesToBase64(new TextEncoder().encode(data)) : bytesToBase64(data);
  }

  function flashSaved() {
    setSavedFlash(true);
    setTimeout(() => setSavedFlash(false), 1600);
  }

  async function save() {
    if (saving || loading || previewOnly) return;
    if (!doc.filePath && !doc.absolutePath) return saveAs(doc.kind);
    setSaving(true);
    try {
      const data = await exportAs(doc.kind);
      let snapshot: FileSnapshot | null | undefined;
      let relativePath: string | null | undefined = doc.filePath;
      let absolutePath = doc.absolutePath;
      if (doc.filePath && projectId) {
        snapshot =
          typeof data === "string"
            ? await BackendAPI.writeProjectFile(projectId, doc.filePath, data)
            : await BackendAPI.writeProjectFileBase64(projectId, doc.filePath, bytesToBase64(data));
      } else if (doc.absolutePath) {
        const res = await BackendAPI.saveFileToPath(projectId, doc.absolutePath, toBase64(data));
        snapshot = res.snapshot;
        relativePath = res.relativePath;
        absolutePath = res.absolutePath;
      }
      setDirty(false);
      flashSaved();
      onSaved({ kind: doc.kind, title: doc.title, snapshot, relativePath, absolutePath, replacesCurrent: true });
    } catch (e) {
      onError(`저장하지 못했습니다: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSaving(false);
    }
  }

  async function saveAs(target: DocKind) {
    setSaveAsOpen(false);
    if (saving || loading) return;
    const currentName = (doc.filePath || doc.absolutePath || doc.title).split(/[\\/]/).pop() || doc.title;
    const origExt = target === "image" ? extensionOf(currentName) : "";
    const ext = origExt || KIND_EXTENSION[target];
    const defaultName = origExt ? currentName : replaceExtension(currentName, target);
    const initialDir = doc.absolutePath
      ? dirOf(doc.absolutePath)
      : projectRoot
        ? doc.filePath
          ? dirOf(joinPath(projectRoot, doc.filePath))
          : projectRoot
        : undefined;
    try {
      const picked = await BackendAPI.pickSavePath(
        initialDir,
        defaultName,
        `${KIND_LABEL[target]} (*.${ext})|*.${ext}|모든 파일 (*.*)|*.*`
      );
      if (!picked) return;
      setSaving(true);
      const data = await exportAs(target);
      const res = await BackendAPI.saveFileToPath(projectId, picked, toBase64(data));
      const replacesCurrent = target === doc.kind;
      if (replacesCurrent) setDirty(false);
      flashSaved();
      onSaved({
        kind: target,
        title: picked.split(/[\\/]/).pop() || picked,
        snapshot: res.snapshot,
        relativePath: res.relativePath,
        absolutePath: res.absolutePath,
        replacesCurrent,
      });
    } catch (e) {
      onError(`저장하지 못했습니다: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSaving(false);
    }
  }

  // Ctrl+S 저장
  const saveRef = useRef(save);
  saveRef.current = save;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        saveRef.current();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  async function copyContent() {
    try {
      await navigator.clipboard.writeText(await currentMarkdown());
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      onError(String(e));
    }
  }

  function requestClose() {
    if (dirty) setConfirmClose(true);
    else onClose();
  }

  const absoluteForAttach = doc.absolutePath || (projectRoot && doc.filePath ? joinPath(projectRoot, doc.filePath) : undefined);
  const notice =
    doc.kind === "docx"
      ? "Word 문서를 서식 편집기로 열었습니다. 저장하면 글꼴·색상·머리글/바닥글 같은 복잡한 서식은 단순화됩니다. 저장 전 상태는 위의 [변경 기록]에서 비교하고 되돌릴 수 있습니다."
      : doc.kind === "hwpx" && hwpxMode === "engine"
        ? "한글 문서를 원래 모양 그대로 열었습니다. 글자를 눌러 바로 고칠 수 있고(입력·지우기·Enter·Ctrl+Z), 저장해도 서식은 그대로입니다. 글자 모양이나 표 구조를 바꾸려면 [한글에서 열기]를 쓰거나 엑사원에게 요청하세요."
        : doc.kind === "hwpx"
          ? "한글 문서 엔진을 쓸 수 없어 단순 편집기로 열었습니다. 저장하면 용지 크기·여백은 유지되고, 글꼴·글상자·머리말/꼬리말·각주 같은 복잡한 서식은 단순화됩니다. 서식을 지키며 문구만 고치려면 엑사원에게 요청하세요."
      : doc.kind === "pptx"
        ? `PowerPoint 파일은 미리보기만 합니다. 고치려면 ${onOpenExternal ? "[PowerPoint에서 열기]를 누르거나 " : ""}엑사원에게 바꿀 내용을 요청하세요.`
        : doc.kind === "xlsx"
          ? `Excel 파일은 미리보기만 합니다. 고치려면 ${onOpenExternal ? "[Excel에서 열기]를 누르거나 " : ""}엑사원에게 바꿀 내용을 요청하세요.`
          : doc.kind === "pdf"
            ? "PDF는 읽기 전용입니다. [다른 이름으로 저장]에서 Markdown 텍스트로 내보낼 수 있습니다."
            : null;

  const modeOptions: { value: Mode; label: string }[] =
    doc.kind === "markdown"
      ? [
          { value: "rich", label: "서식" },
          { value: "source", label: "원문" },
        ]
      : doc.kind === "html"
        ? [
            { value: "preview", label: "미리보기" },
            { value: "source", label: "원문" },
          ]
        : [];

  return (
    <div className="flex flex-col h-full min-h-0">
      {/* 헤더 */}
      <div className="px-3 py-2 border-b border-[#E5E3DB] dark:border-[#343430] flex items-center gap-2 bg-[#F6F5F0] dark:bg-[#1B1B19]">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <span className="text-[10px] font-bold px-1.5 py-0.5 rounded bg-[#A50034]/10 text-[#A50034] dark:bg-[#FF5C8D]/15 dark:text-[#FF5C8D] shrink-0">
              {KIND_LABEL[doc.kind]}
            </span>
            <span className="text-xs font-bold text-stone-900 dark:text-stone-100 truncate">{doc.title}</span>
            {dirty && <span className="w-1.5 h-1.5 rounded-full bg-amber-500 shrink-0" title="저장하지 않은 변경 사항" />}
          </div>
          {(doc.filePath || doc.absolutePath) && (
            <div className="text-[11px] text-stone-500 dark:text-stone-400 font-mono truncate">
              {doc.filePath || doc.absolutePath}
            </div>
          )}
        </div>

        {modeOptions.length > 0 && (
          <div className="flex bg-stone-200/80 dark:bg-[#2A2A27] p-0.5 rounded-lg text-xs shrink-0">
            {modeOptions.map((o) => (
              <button
                key={o.value}
                onClick={() => switchMode(o.value)}
                className={`px-2 py-1 rounded-md font-semibold cursor-pointer ${
                  mode === o.value
                    ? "bg-white dark:bg-[#383834] text-stone-900 dark:text-stone-100 shadow-2xs"
                    : "text-stone-600 dark:text-stone-400"
                }`}
              >
                {o.label}
              </button>
            ))}
          </div>
        )}

        <div className="flex items-center gap-0.5 shrink-0 text-stone-600 dark:text-stone-300">
          {externalApp && onOpenExternal && (
            <button
              onClick={onOpenExternal}
              className="px-2 py-1 rounded-lg hover:bg-stone-200 dark:hover:bg-[#32322F] cursor-pointer flex items-center gap-1 text-xs font-semibold"
              title={`${externalApp}에서 열어 고치기`}
            >
              <ExternalLink className="w-4 h-4" />
              <span className="hidden xl:inline">{externalApp === "기본 앱" ? "기본 앱으로 열기" : `${externalApp}에서 열기`}</span>
            </button>
          )}
          {!previewOnly && (
            <button
              onClick={save}
              disabled={saving || loading}
              className="px-2 py-1 rounded-lg hover:bg-stone-200 dark:hover:bg-[#32322F] cursor-pointer disabled:opacity-40 flex items-center gap-1 text-xs font-semibold"
              title="저장 (Ctrl+S)"
            >
              {savedFlash ? <Check className="w-4 h-4 text-emerald-600" /> : <Save className="w-4 h-4" />}
              <span className="hidden xl:inline">{saving ? "저장 중" : "저장"}</span>
            </button>
          )}
          <div className="relative">
            <button
              onClick={() => setSaveAsOpen((v) => !v)}
              disabled={saving || loading}
              className="px-1.5 py-1 rounded-lg hover:bg-stone-200 dark:hover:bg-[#32322F] cursor-pointer disabled:opacity-40 flex items-center"
              title="다른 이름으로 저장 / 형식 변환"
            >
              <SaveAll className="w-4 h-4" />
              <ChevronDown className="w-3 h-3" />
            </button>
            {saveAsOpen && (
              <div className="absolute right-0 top-full mt-1 z-20 w-52 rounded-xl border border-stone-200 dark:border-[#3A3A36] bg-white dark:bg-[#262624] shadow-lg py-1 text-xs">
                <div className="px-3 py-1 text-stone-400">다른 이름으로 저장</div>
                {SAVE_AS_TARGETS[doc.kind].map((t) => {
                  const optExt = (t === "image" ? extensionOf(doc.title) : "") || KIND_EXTENSION[t];
                  return (
                    <button
                      key={t}
                      onClick={() => saveAs(t)}
                      className="w-full text-left px-3 py-1.5 hover:bg-stone-100 dark:hover:bg-[#32322F] text-stone-800 dark:text-stone-100 cursor-pointer"
                    >
                      {KIND_LABEL[t]} <span className="text-stone-400">(.{optExt})</span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
          {onRunPython && (
            <button
              onClick={async () => {
                // 고친 내용이 있으면 저장한 뒤 실행한다.
                if (dirty) await save();
                onRunPython();
              }}
              disabled={saving || loading}
              className="px-2 py-1 rounded-lg bg-[#A50034] hover:bg-[#8A002B] text-white cursor-pointer disabled:opacity-40 flex items-center gap-1 text-xs font-semibold"
              title="Python으로 실행"
            >
              <Play className="w-3.5 h-3.5" />
              <span className="hidden xl:inline">실행</span>
            </button>
          )}
          {onShowHistory && historyCount > 0 && (
            <button
              onClick={onShowHistory}
              className="px-1.5 py-1 rounded-lg hover:bg-stone-200 dark:hover:bg-[#32322F] cursor-pointer flex items-center gap-0.5"
              title={`변경 기록 ${historyCount}개 — 바뀐 내용 비교·되돌리기`}
              aria-label={`변경 기록 ${historyCount}개`}
            >
              <History className="w-4 h-4" />
              <span className="text-[10px] font-semibold tabular-nums">{historyCount}</span>
            </button>
          )}
          {doc.kind !== "image" && (
            <button onClick={copyContent} className="p-1.5 rounded-lg hover:bg-stone-200 dark:hover:bg-[#32322F] cursor-pointer" title="내용 복사 (Markdown)">
              {copied ? <Check className="w-4 h-4 text-emerald-600" /> : <Copy className="w-4 h-4" />}
            </button>
          )}
          {onAttach && absoluteForAttach && (
            <button onClick={() => onAttach(absoluteForAttach)} className="p-1.5 rounded-lg hover:bg-stone-200 dark:hover:bg-[#32322F] cursor-pointer" title="채팅에 첨부">
              <Paperclip className="w-4 h-4" />
            </button>
          )}
          <button onClick={onToggleExpand} className="p-1.5 rounded-lg hover:bg-stone-200 dark:hover:bg-[#32322F] cursor-pointer" title={expanded ? "패널 좁게" : "패널 넓게"}>
            {expanded ? <Minimize2 className="w-4 h-4" /> : <Maximize2 className="w-4 h-4" />}
          </button>
          <button onClick={requestClose} className="p-1.5 rounded-lg hover:bg-stone-200 dark:hover:bg-[#32322F] cursor-pointer" title="닫기">
            <X className="w-4 h-4" />
          </button>
        </div>
      </div>

      {confirmClose && (
        <div className="px-3 py-2 bg-amber-50 dark:bg-amber-950/40 border-b border-amber-200 dark:border-amber-900 text-xs text-amber-900 dark:text-amber-200 flex items-center gap-2">
          <span className="flex-1">저장하지 않은 변경 사항이 있습니다.</span>
          <button
            onClick={async () => {
              await save();
              onClose();
            }}
            className="px-2 py-1 rounded-md bg-[#A50034] text-white font-semibold cursor-pointer"
          >
            저장 후 닫기
          </button>
          <button onClick={onClose} className="px-2 py-1 rounded-md bg-stone-200 dark:bg-[#32322F] font-semibold cursor-pointer">
            버리기
          </button>
          <button onClick={() => setConfirmClose(false)} className="px-2 py-1 rounded-md font-semibold cursor-pointer">
            취소
          </button>
        </div>
      )}

      {notice && !noticeHidden && !loading && !loadError && (
        <div className="px-3 py-1.5 bg-sky-50 dark:bg-sky-950/30 border-b border-sky-100 dark:border-sky-900/50 text-[11px] text-sky-800 dark:text-sky-200 flex items-start gap-1.5">
          <Info className="w-3.5 h-3.5 mt-px shrink-0" />
          <span className="flex-1">{notice}</span>
          <button onClick={() => setNoticeHidden(true)} className="cursor-pointer">
            <X className="w-3 h-3" />
          </button>
        </div>
      )}

      {/* 본문 */}
      <div className="flex-1 min-h-0">
        {loading ? (
          <div className="p-6 text-xs text-stone-500">문서를 여는 중...</div>
        ) : loadError ? (
          <div className="p-6 text-xs text-red-600 dark:text-red-400">{loadError}</div>
        ) : doc.kind === "pdf" ? (
          <PdfViewer bytes={doc.bytes ?? new Uint8Array()} onError={onError} />
        ) : doc.kind === "pptx" && pptx ? (
          <PptxViewer deck={pptx.deck} />
        ) : doc.kind === "xlsx" ? (
          <XlsxViewer bytes={doc.bytes ?? new Uint8Array()} onError={onError} />
        ) : doc.kind === "image" ? (
          <ImageViewer bytes={doc.bytes ?? new Uint8Array()} title={doc.title} onError={onError} />
        ) : doc.kind === "hwpx" && hwpxMode === "engine" ? (
          <HwpxEditor
            ref={hwpxRef}
            bytes={doc.bytes ?? new Uint8Array()}
            onDirty={markDirty}
            onError={onError}
            onLoadFailed={fallbackToSimpleHwpx}
          />
        ) : (doc.kind === "markdown" || doc.kind === "docx" || doc.kind === "hwpx") && mode === "rich" ? (
          <RichTextEditor
            key={`${doc.id}:${editorKey}`}
            initialHtml={initialHtml}
            variant={doc.kind === "markdown" ? "plain" : "page"}
            onChange={(html) => {
              htmlRef.current = html;
              markDirty();
            }}
          />
        ) : doc.kind === "html" && mode === "preview" ? (
          <HtmlPreview key={doc.id} doc={doc} projectId={projectId} html={sourceText} />
        ) : (
          <textarea
            value={sourceText}
            onChange={(e) => {
              textRef.current = e.target.value;
              setSourceText(e.target.value);
              markDirty();
            }}
            spellCheck={false}
            className="w-full h-full p-5 font-mono text-xs leading-relaxed text-stone-800 dark:text-stone-100 bg-transparent focus:outline-none resize-none"
          />
        )}
      </div>
    </div>
  );
}
