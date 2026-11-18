import type { CanvasDoc } from "./canvasDoc";

// The preview URL has its own loopback origin, separate from the Tauri app.
export const HTML_PREVIEW_SANDBOX = "allow-scripts allow-same-origin";

export interface HtmlPreviewRequest {
  projectId: string | null;
  relativePath: string | null;
  absolutePath: string | null;
  html: string;
}

export function htmlPreviewRequest(doc: Pick<CanvasDoc, "filePath" | "absolutePath">, projectId: string | null, html: string): HtmlPreviewRequest {
  return {
    projectId: doc.filePath ? projectId : null,
    relativePath: doc.filePath ?? null,
    absolutePath: doc.absolutePath ?? null,
    html,
  };
}
