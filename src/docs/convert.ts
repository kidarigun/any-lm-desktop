import { base64ToBytes, bytesToBase64 } from "./base64";
import { docxToMarkdown, markdownToDocx } from "./docx";
import { hwpxToMarkdown, markdownToHwpx, replaceTextInHwpx } from "./hwpx";
import { stripInlineMarkdown } from "./markdown";
import { extractPdfText } from "./pdf";
import { markdownToPptx, pptxToMarkdown, replaceTextInOoxml } from "./pptx";
import { markdownToXlsx, replaceTextInXlsx, xlsxToMarkdown } from "./xlsx";

/** Rust 백엔드가 보내는 문서 변환 요청 (src-tauri/src/doc_bridge.rs와 형식이 같아야 한다) */
export interface DocBridgeRequest {
  requestId: string;
  op: "extract" | "generate" | "replace";
  kind: "docx" | "pptx" | "pdf" | "xlsx" | "hwpx";
  dataBase64?: string | null;
  markdown?: string | null;
  title?: string | null;
  target?: string | null;
  replacement?: string | null;
}

export interface DocBridgeReply {
  ok: boolean;
  text?: string;
  dataBase64?: string;
  error?: string;
}

export async function handleDocBridgeRequest(req: DocBridgeRequest): Promise<DocBridgeReply> {
  try {
    switch (req.op) {
      case "extract": {
        const bytes = base64ToBytes(req.dataBase64 ?? "");
        const text =
          req.kind === "docx"
            ? await docxToMarkdown(bytes)
            : req.kind === "pptx"
              ? await pptxToMarkdown(bytes)
              : req.kind === "xlsx"
                ? await xlsxToMarkdown(bytes)
                : req.kind === "hwpx"
                  ? await hwpxToMarkdown(bytes)
                  : await extractPdfText(bytes);
        return { ok: true, text };
      }
      case "generate": {
        const md = req.markdown ?? "";
        const title = req.title || "문서";
        if (req.kind === "pdf") throw new Error("PDF 생성은 지원하지 않습니다. .docx 또는 .md로 작성하세요.");
        const bytes =
          req.kind === "docx"
            ? await markdownToDocx(md, title)
            : req.kind === "xlsx"
              ? await markdownToXlsx(md, title)
              : req.kind === "hwpx"
                ? await markdownToHwpx(md, title)
                : await markdownToPptx(md, title);
        return { ok: true, dataBase64: bytesToBase64(bytes) };
      }
      case "replace": {
        if (req.kind === "pdf") throw new Error("PDF는 수정할 수 없습니다.");
        const bytes = base64ToBytes(req.dataBase64 ?? "");
        const target = req.target ?? "";
        const replacement = req.replacement ?? "";
        if (req.kind === "xlsx") {
          // 엑셀 칸 값에는 Markdown 기호가 없으므로 그대로 찾는다.
          return { ok: true, dataBase64: bytesToBase64(await replaceTextInXlsx(bytes, target, replacement)) };
        }
        const kind = req.kind;
        const replace = (t: string, r: string) =>
          kind === "hwpx" ? replaceTextInHwpx(bytes, t, r) : replaceTextInOoxml(bytes, kind, t, r);
        let out: Uint8Array;
        try {
          out = await replace(target, replacement);
        } catch (first) {
          // AI가 read_file 결과(Markdown)의 기호까지 target에 넣은 경우를 한 번 더 시도한다.
          const plainTarget = stripInlineMarkdown(target);
          if (plainTarget === target) throw first;
          out = await replace(plainTarget, stripInlineMarkdown(replacement));
        }
        return { ok: true, dataBase64: bytesToBase64(out) };
      }
      default:
        throw new Error(`알 수 없는 요청입니다: ${(req as { op: string }).op}`);
    }
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
}
