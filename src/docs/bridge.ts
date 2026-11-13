import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DocBridgeRequest } from "./convert";

const REQUEST_EVENT = "doc-bridge-request";

/**
 * Rust 에이전트가 DOCX/PPTX/PDF를 읽거나 만들 때 보내는 요청을 받아 처리한다.
 * 문서 변환 라이브러리(mammoth, docx, pptxgenjs, pdf.js)는 웹뷰에서 실행된다.
 */
export async function startDocBridge(): Promise<UnlistenFn> {
  const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  if (!isTauri) return () => {};
  return listen<DocBridgeRequest>(REQUEST_EVENT, async (event) => {
    const req = event.payload;
    // 변환 라이브러리는 처음 요청이 올 때 불러온다.
    const { handleDocBridgeRequest } = await import("./convert");
    const reply = await handleDocBridgeRequest(req);
    try {
      await invoke("doc_bridge_respond", { requestId: req.requestId, reply });
    } catch (e) {
      console.error("doc_bridge_respond 실패", e);
    }
  });
}
