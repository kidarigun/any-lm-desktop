import { useEffect, useState } from "react";
import { BackendAPI, isTauri } from "../api";
import type { CanvasDoc } from "./canvasDoc";
import { HTML_PREVIEW_SANDBOX, htmlPreviewRequest } from "./htmlPreview";

interface HtmlPreviewProps {
  doc: CanvasDoc;
  projectId: string | null;
  html: string;
}

/** 실제 브라우저 origin에서 HTML과 같은 폴더의 CSS·JS·이미지 등을 불러오는 격리 미리보기. */
export function HtmlPreview({ doc, projectId, html }: HtmlPreviewProps) {
  const { filePath, absolutePath } = doc;
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let token: string | undefined;
    if (!isTauri()) return;
    setUrl(null);
    setError(null);
    const release = (id: string) => void BackendAPI.closeHtmlPreview(id).catch(console.warn);
    BackendAPI.prepareHtmlPreview(htmlPreviewRequest({ filePath, absolutePath }, projectId, html))
      .then((result) => {
        if (cancelled) {
          release(result.token);
        } else {
          token = result.token;
          setUrl(result.url);
        }
      })
      .catch((error) => {
        if (!cancelled) setError(`HTML 미리보기를 준비할 수 없습니다: ${String(error)}`);
      });
    return () => {
      cancelled = true;
      if (token) release(token);
    };
  }, [filePath, absolutePath, html, projectId]);

  if (error) return <div role="alert" className="p-6 text-xs text-red-600 dark:text-red-400">{error}</div>;
  if (isTauri() && !url) return <div className="p-6 text-xs text-stone-500">웹 미리보기를 준비하는 중...</div>;
  return (
    <iframe
      title={doc.title}
      sandbox={url ? HTML_PREVIEW_SANDBOX : ""}
      src={url ?? undefined}
      srcDoc={url ? undefined : html}
      className="w-full h-full bg-white border-0"
    />
  );
}
