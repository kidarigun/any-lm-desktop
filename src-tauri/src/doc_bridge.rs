//! Rust 에이전트가 DOCX/HWPX/PPTX/PDF/XLSX를 읽거나 만들 때 웹뷰(프론트엔드)의 변환 라이브러리를 호출하는 다리.
//!
//! 요청은 `doc-bridge-request` 이벤트로 보내고, 프론트엔드가 `doc_bridge_respond` 명령으로 답한다.
//! 프론트엔드 쪽 구현: `src/docs/bridge.ts`, `src/docs/convert.ts`.

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::oneshot;
use uuid::Uuid;

pub const REQUEST_EVENT: &str = "doc-bridge-request";
const BRIDGE_TIMEOUT: Duration = Duration::from_secs(90);

/// 웹뷰 변환기가 처리하는 문서 형식
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    Docx,
    Hwpx,
    Pptx,
    Pdf,
    Xlsx,
}

impl DocKind {
    pub fn from_path(path: &str) -> Option<DocKind> {
        let ext = Path::new(path)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())?;
        match ext.as_str() {
            "docx" => Some(DocKind::Docx),
            "hwpx" => Some(DocKind::Hwpx),
            "pptx" => Some(DocKind::Pptx),
            "pdf" => Some(DocKind::Pdf),
            "xlsx" => Some(DocKind::Xlsx),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DocKind::Docx => "Word(DOCX)",
            DocKind::Hwpx => "한글(HWPX)",
            DocKind::Pptx => "PowerPoint(PPTX)",
            DocKind::Pdf => "PDF",
            DocKind::Xlsx => "Excel(XLSX)",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeRequest {
    request_id: String,
    op: &'static str,
    kind: DocKind,
    data_base64: Option<String>,
    markdown: Option<String>,
    title: Option<String>,
    target: Option<String>,
    replacement: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeReply {
    pub ok: bool,
    pub text: Option<String>,
    pub data_base64: Option<String>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct DocBridge {
    pending: Mutex<HashMap<String, oneshot::Sender<BridgeReply>>>,
}

impl DocBridge {
    /// 프론트엔드의 응답을 기다리던 요청에 전달한다.
    pub fn respond(&self, request_id: &str, reply: BridgeReply) -> Result<(), String> {
        let sender = self
            .pending
            .lock()
            .map_err(|e| e.to_string())?
            .remove(request_id)
            .ok_or_else(|| "이미 처리되었거나 알 수 없는 문서 변환 요청입니다.".to_string())?;
        let _ = sender.send(reply);
        Ok(())
    }
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

const ERR_UNAVAILABLE: &str = "문서 변환기를 사용할 수 없습니다 (앱 창이 준비되지 않았습니다).";
const ERR_SEND_PREFIX: &str = "문서 변환 요청을 보내지 못했습니다";
const ERR_CLOSED: &str = "문서 변환기가 응답 없이 종료되었습니다.";
const ERR_TIMEOUT_PREFIX: &str = "문서 변환이 ";
const ERR_TIMEOUT_SUFFIX: &str = "초 안에 끝나지 않았습니다.";

/// 다리 자체의 일시적 오류(변환기 없음·보내기 실패·응답 없이 종료·시간 초과)인지.
/// 이런 오류는 파일 탓이 아니므로 다음에 다시 시도할 수 있다. 변환기가 돌려준 변환 실패는 아니다.
pub fn is_transient_error(err: &str) -> bool {
    err == ERR_UNAVAILABLE
        || err == ERR_CLOSED
        || err.starts_with(ERR_SEND_PREFIX)
        || (err.starts_with(ERR_TIMEOUT_PREFIX) && err.ends_with(ERR_TIMEOUT_SUFFIX))
}

async fn request<R: Runtime>(app: &AppHandle<R>, mut req: BridgeRequest) -> Result<BridgeReply, String> {
    let bridge = app.try_state::<DocBridge>().ok_or_else(|| ERR_UNAVAILABLE.to_string())?;
    let id = Uuid::new_v4().to_string();
    req.request_id = id.clone();
    let (tx, rx) = oneshot::channel();
    bridge
        .pending
        .lock()
        .map_err(|e| e.to_string())?
        .insert(id.clone(), tx);

    if let Err(e) = app.emit(REQUEST_EVENT, req) {
        bridge.pending.lock().map_err(|e| e.to_string())?.remove(&id);
        return Err(format!("{}: {}", ERR_SEND_PREFIX, e));
    }

    let result = tokio::time::timeout(BRIDGE_TIMEOUT, rx).await;
    bridge.pending.lock().map_err(|e| e.to_string())?.remove(&id);
    let reply = match result {
        Ok(Ok(reply)) => reply,
        Ok(Err(_)) => return Err(ERR_CLOSED.to_string()),
        Err(_) => return Err(format!("{}{}{}", ERR_TIMEOUT_PREFIX, BRIDGE_TIMEOUT.as_secs(), ERR_TIMEOUT_SUFFIX)),
    };
    if reply.ok {
        Ok(reply)
    } else {
        Err(reply.error.unwrap_or_else(|| "문서 변환에 실패했습니다.".to_string()))
    }
}

fn empty(op: &'static str, kind: DocKind) -> BridgeRequest {
    BridgeRequest {
        request_id: String::new(),
        op,
        kind,
        data_base64: None,
        markdown: None,
        title: None,
        target: None,
        replacement: None,
    }
}

/// 문서 바이트를 AI가 읽을 수 있는 Markdown 텍스트로 바꾼다.
pub async fn extract_text<R: Runtime>(app: &AppHandle<R>, kind: DocKind, bytes: &[u8]) -> Result<String, String> {
    let mut req = empty("extract", kind);
    req.data_base64 = Some(b64().encode(bytes));
    Ok(request(app, req).await?.text.unwrap_or_default())
}

/// Markdown으로 DOCX/HWPX/PPTX/XLSX 문서를 만든다.
pub async fn generate<R: Runtime>(
    app: &AppHandle<R>,
    kind: DocKind,
    markdown: &str,
    title: &str,
) -> Result<Vec<u8>, String> {
    let mut req = empty("generate", kind);
    req.markdown = Some(markdown.to_string());
    req.title = Some(title.to_string());
    decode(request(app, req).await?)
}

/// DOCX/HWPX/PPTX/XLSX 안의 문구(엑셀은 칸 값·수식)를 서식을 유지한 채 교체한다.
pub async fn replace_text<R: Runtime>(
    app: &AppHandle<R>,
    kind: DocKind,
    bytes: &[u8],
    target: &str,
    replacement: &str,
) -> Result<Vec<u8>, String> {
    let mut req = empty("replace", kind);
    req.data_base64 = Some(b64().encode(bytes));
    req.target = Some(target.to_string());
    req.replacement = Some(replacement.to_string());
    decode(request(app, req).await?)
}

fn decode(reply: BridgeReply) -> Result<Vec<u8>, String> {
    let data = reply
        .data_base64
        .ok_or_else(|| "문서 변환 결과가 비어 있습니다.".to_string())?;
    b64()
        .decode(data)
        .map_err(|e| format!("문서 변환 결과를 해석할 수 없습니다: {}", e))
}

pub fn encode_base64(bytes: &[u8]) -> String {
    b64().encode(bytes)
}

pub fn decode_base64(data: &str) -> Result<Vec<u8>, String> {
    b64()
        .decode(data)
        .map_err(|e| format!("파일 데이터를 해석할 수 없습니다: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_kinds_case_insensitively() {
        assert_eq!(DocKind::from_path("docs/보고서.DOCX"), Some(DocKind::Docx));
        assert_eq!(DocKind::from_path("a.pptx"), Some(DocKind::Pptx));
        assert_eq!(DocKind::from_path("scan.pdf"), Some(DocKind::Pdf));
        assert_eq!(DocKind::from_path("out/매출.XLSX"), Some(DocKind::Xlsx));
        assert_eq!(DocKind::from_path("공문/시행문.HWPX"), Some(DocKind::Hwpx));
        assert_eq!(DocKind::from_path("옛문서.hwp"), None);
        assert_eq!(DocKind::from_path("old.xls"), None);
        assert_eq!(DocKind::from_path("notes.md"), None);
        assert_eq!(DocKind::from_path("noext"), None);
    }

    #[tokio::test]
    async fn request_round_trip_through_respond() {
        let app = tauri::test::mock_app();
        app.manage(DocBridge::default());
        let handle = app.handle().clone();

        // 프론트엔드 역할: 요청 이벤트를 받으면 바로 응답한다.
        use tauri::Listener;
        let responder = handle.clone();
        handle.listen(REQUEST_EVENT, move |event| {
            let v: serde_json::Value = serde_json::from_str(event.payload()).unwrap();
            assert_eq!(v["op"], "extract");
            assert_eq!(v["kind"], "pdf");
            let id = v["requestId"].as_str().unwrap().to_string();
            responder
                .state::<DocBridge>()
                .respond(
                    &id,
                    BridgeReply { ok: true, text: Some("## 페이지 1\n안녕".into()), ..Default::default() },
                )
                .unwrap();
        });

        let text = extract_text(&handle, DocKind::Pdf, b"%PDF-1.4").await.unwrap();
        assert!(text.contains("안녕"));
        assert!(handle.state::<DocBridge>().pending.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_reply_becomes_error() {
        let app = tauri::test::mock_app();
        app.manage(DocBridge::default());
        let handle = app.handle().clone();
        use tauri::Listener;
        let responder = handle.clone();
        handle.listen(REQUEST_EVENT, move |event| {
            let v: serde_json::Value = serde_json::from_str(event.payload()).unwrap();
            let id = v["requestId"].as_str().unwrap().to_string();
            responder
                .state::<DocBridge>()
                .respond(&id, BridgeReply { ok: false, error: Some("변환 실패".into()), ..Default::default() })
                .unwrap();
        });
        let err = generate(&handle, DocKind::Docx, "# x", "t").await.unwrap_err();
        assert_eq!(err, "변환 실패");
    }

    #[tokio::test]
    async fn missing_bridge_is_reported() {
        let app = tauri::test::mock_app();
        let err = extract_text(app.handle(), DocKind::Docx, b"x").await.unwrap_err();
        assert!(err.contains("문서 변환기"));
    }

    #[tokio::test]
    async fn bridge_errors_are_classified_as_transient() {
        let app = tauri::test::mock_app();
        // 다리가 없으면 일시적 오류
        let err = extract_text(app.handle(), DocKind::Pdf, b"x").await.unwrap_err();
        assert!(is_transient_error(&err), "{err}");
        assert!(is_transient_error(&format!("문서 변환이 {}초 안에 끝나지 않았습니다.", BRIDGE_TIMEOUT.as_secs())));
        assert!(is_transient_error("문서 변환기가 응답 없이 종료되었습니다."));
        assert!(is_transient_error("문서 변환 요청을 보내지 못했습니다: closed"));
        // 변환기가 돌려준 실패(파일 탓)는 아니다
        assert!(!is_transient_error("문서 변환에 실패했습니다."));
        assert!(!is_transient_error("PDF를 읽을 수 없습니다: Invalid PDF structure"));
    }
}
