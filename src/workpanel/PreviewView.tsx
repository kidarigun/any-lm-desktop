import { useEffect, useState, type FormEvent } from "react";
import { ArrowLeft, ArrowRight, ExternalLink, RefreshCw } from "lucide-react";
import { BackendAPI } from "../api";
import { DevServerAPI, previewDestination, previewFrameUrl } from "./devServerApi";

const iconButton = "p-1.5 rounded hover:bg-stone-100 dark:hover:bg-stone-700 disabled:opacity-40";

export function PreviewView({ initialUrl }: { initialUrl: string }) {
  const [history, setHistory] = useState({ urls: [initialUrl], index: 0 });
  const current = history.urls[history.index];
  const frameUrl = previewFrameUrl(current);
  const [input, setInput] = useState(initialUrl);
  const [width, setWidth] = useState("전체");
  const [refresh, setRefresh] = useState(0);
  const [notice, setNotice] = useState("");
  const [failure, setFailure] = useState("");

  useEffect(() => {
    let currentRequest = true;
    setFailure("");
    if (!frameUrl) return;
    void DevServerAPI.checkPreview(frameUrl).catch((error) => {
      if (currentRequest) setFailure(String(error));
    });
    return () => { currentRequest = false; };
  }, [frameUrl, refresh]);

  function navigate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setNotice("");
    const target = previewDestination(input);
    if (!target) {
      setNotice("올바른 HTTP 또는 HTTPS 주소를 입력해 주세요.");
      return;
    }
    if (!target.local) {
      setNotice("외부 사이트는 브라우저에서 엽니다.");
      void BackendAPI.openExternalUrl(target.url).catch((e) => setNotice(String(e)));
      return;
    }
    setInput(target.url);
    if (current === target.url) {
      setRefresh((n) => n + 1);
      return;
    }
    setHistory((h) => ({ urls: [...h.urls.slice(0, h.index + 1), target.url], index: h.index + 1 }));
  }

  function step(offset: -1 | 1) {
    const index = history.index + offset;
    if (index < 0 || index >= history.urls.length) return;
    setHistory({ ...history, index });
    setInput(history.urls[index]);
    setNotice("");
  }

  return (
    <div className="flex h-full min-w-0 flex-col bg-stone-50 dark:bg-[#232321]">
      <form onSubmit={navigate} className="flex items-center gap-1 border-b border-stone-200 p-2 dark:border-stone-700">
        <button type="button" title="뒤로" aria-label="뒤로" disabled={history.index === 0} onClick={() => step(-1)} className={iconButton}><ArrowLeft size={15} /></button>
        <button type="button" title="앞으로" aria-label="앞으로" disabled={history.index + 1 === history.urls.length} onClick={() => step(1)} className={iconButton}><ArrowRight size={15} /></button>
        <button type="button" title="새로고침" aria-label="새로고침" onClick={() => setRefresh((n) => n + 1)} className={iconButton}><RefreshCw size={15} /></button>
        <input aria-label="미리보기 주소" placeholder="http://localhost:3000" value={input} onChange={(e) => setInput(e.target.value)}
          className="min-w-0 flex-1 rounded border border-stone-300 bg-white px-2 py-1 text-xs dark:border-stone-600 dark:bg-stone-800" />
        <button type="submit" className={`${iconButton} text-xs`}>열기</button>
        <button type="button" title="브라우저로 열기" aria-label="브라우저로 열기" disabled={!current}
          onClick={() => void BackendAPI.openExternalUrl(current).catch((e) => setNotice(String(e)))} className={iconButton}><ExternalLink size={15} /></button>
      </form>
      <div className="flex items-center justify-between gap-2 px-2 py-1 text-xs">
        <label className="flex items-center gap-1">
          화면 폭
          <select value={width} onChange={(e) => setWidth(e.target.value)}
            className="rounded border border-stone-300 bg-white p-1 dark:border-stone-600 dark:bg-stone-800">
            <option value="전체">전체</option>
            <option value="768">태블릿 768px</option>
            <option value="375">모바일 375px</option>
          </select>
        </label>
        {(notice || failure) && <span role="status" className="min-w-0 break-words">{notice || failure}</span>}
      </div>
      <div className="min-h-0 flex-1 overflow-auto bg-stone-200/50 dark:bg-stone-900">
        {frameUrl ? <iframe
          key={`${current}-${refresh}`}
          title="개발 서버 미리보기"
          src={frameUrl}
          sandbox="allow-scripts allow-same-origin allow-forms"
          referrerPolicy="no-referrer"
          style={{ width: width === "전체" ? "100%" : Number(width) }}
          className="mx-auto block h-full border-0 bg-white"
        /> : <p className="p-4 text-center text-sm text-stone-500 dark:text-stone-300">
          {current ? "이 주소는 [브라우저로 열기]에서 확인할 수 있습니다." : "미리 볼 로컬 서버 주소를 입력해 주세요."}
        </p>}
      </div>
    </div>
  );
}
