import type { PDFDocumentProxy } from "pdfjs-dist";

type PdfJs = typeof import("pdfjs-dist");

/**
 * pdf.js 보조 파일(CMap, 표준 글꼴, wasm) 위치.
 * 앱에서는 vite.config.ts의 pdfjsAssets 플러그인이 `/pdfjs/` 경로로 제공한다.
 * 한글 PDF는 CMap이 있어야 글자를 제대로 읽을 수 있다.
 */
let assetBase = "/pdfjs/";
let workerSrcOverride: string | null = null;
let pdfjsPromise: Promise<PdfJs> | null = null;

/** 테스트 등 브라우저가 아닌 환경에서 워커/보조 파일 위치를 지정한다. */
export function configurePdfjs(options: { workerSrc?: string; assetBase?: string }) {
  if (options.workerSrc) workerSrcOverride = options.workerSrc;
  if (options.assetBase) assetBase = options.assetBase.endsWith("/") ? options.assetBase : options.assetBase + "/";
}

async function getPdfjs(): Promise<PdfJs> {
  if (!pdfjsPromise) {
    pdfjsPromise = (async () => {
      const lib = await import("pdfjs-dist");
      if (!lib.GlobalWorkerOptions.workerSrc) {
        lib.GlobalWorkerOptions.workerSrc =
          workerSrcOverride ?? (await import("pdfjs-dist/build/pdf.worker.min.mjs?url")).default;
      }
      return lib;
    })();
  }
  return pdfjsPromise;
}

export interface OpenedPdf {
  doc: PDFDocumentProxy;
  /** 워커와 메모리를 정리한다. */
  close: () => Promise<void>;
}

export async function openPdf(bytes: Uint8Array): Promise<OpenedPdf> {
  const lib = await getPdfjs();
  // pdf.js가 버퍼를 워커로 넘기면서 비우므로 복사본을 넘긴다.
  const task = lib.getDocument({
    data: bytes.slice(),
    cMapUrl: assetBase + "cmaps/",
    cMapPacked: true,
    standardFontDataUrl: assetBase + "standard_fonts/",
    wasmUrl: assetBase + "wasm/",
  });
  const doc = await task.promise;
  return { doc, close: () => task.destroy() };
}

/** PDF의 텍스트를 페이지별로 추출한다 (AI 읽기용). */
export async function extractPdfText(bytes: Uint8Array): Promise<string> {
  const { doc, close } = await openPdf(bytes);
  try {
    const pages: string[] = [];
    for (let i = 1; i <= doc.numPages; i++) {
      const page = await doc.getPage(i);
      const content = await page.getTextContent();
      let text = "";
      for (const item of content.items) {
        if (!("str" in item)) continue;
        text += item.str;
        if (item.hasEOL) text += "\n";
      }
      pages.push(`## 페이지 ${i}\n${text.replace(/[ \t]+\n/g, "\n").trim()}`);
      page.cleanup();
    }
    const body = pages.join("\n\n").trim();
    const hasText = pages.some((p) => p.replace(/^## 페이지 \d+\n?/, "").trim().length > 0);
    return hasText
      ? body + "\n"
      : `${body}\n\n(이 PDF에는 추출할 수 있는 텍스트가 없습니다. 스캔된 이미지 PDF일 수 있습니다.)\n`;
  } finally {
    await close();
  }
}
