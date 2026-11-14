//! 스냅샷 하나가 기록한 변경의 "이전 / 이후" 내용을 텍스트로 꺼낸다 (변경 보기 화면용).
//! docx·pptx·pdf는 문서 변환기로 Markdown을 뽑아 비교할 수 있게 한다.

use crate::doc_bridge::{self, DocKind};
use crate::fs_ops;
use crate::store::FileSnapshot;
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Runtime};

/// 한쪽 내용이 이보다 길면 잘라서 보낸다 (화면에서 비교 계산이 멈추지 않도록).
pub const MAX_DIFF_CHARS: usize = 400_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiff {
    pub snapshot_id: String,
    pub relative_path: String,
    pub action: String,
    pub created_at: String,
    /// `None`: 파일이 없었음(새로 만듦) 또는 텍스트로 볼 수 없음 (`kind`로 구분)
    pub before: Option<String>,
    pub after: Option<String>,
    pub before_exists: bool,
    pub after_exists: bool,
    /// "text" | "document"(문서를 Markdown으로 바꿔 비교) | "binary"(비교 불가)
    pub kind: String,
    /// 화면에 함께 보여 줄 안내 (근사치로 비교했거나 잘랐을 때 등)
    pub notes: Vec<String>,
    /// 이 변경 뒤에 같은 파일을 또 바꾼 기록이 있음 (이 시점으로 되돌리면 이후 변경도 사라진다)
    pub superseded: bool,
    /// "agent" | "user" | 없음(예전 기록)
    pub source: Option<String>,
}

/// 한쪽 내용의 원본
enum Source {
    Missing,
    Text(String),
    Bytes(Vec<u8>),
}

fn same_file(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().trim_start_matches("./").replace('\\', "/").to_lowercase();
    norm(a) == norm(b)
}

/// `snapshots`에서 `snap` 다음에 같은 파일을 바꾼 스냅샷을 찾는다 (기록 순서대로 쌓여 있다).
pub fn next_change_of_same_file<'a>(snapshots: &'a [FileSnapshot], snap: &FileSnapshot) -> Option<&'a FileSnapshot> {
    let idx = snapshots.iter().position(|s| s.id == snap.id)?;
    snapshots[idx + 1..]
        .iter()
        .find(|s| s.project_id == snap.project_id && same_file(&s.relative_path, &snap.relative_path))
}

fn before_source(snap: &FileSnapshot) -> Result<Source, String> {
    if !snap.existed_before {
        return Ok(Source::Missing);
    }
    match &snap.backup_blob {
        Some(name) => fs_ops::load_blob(name).map(Source::Bytes),
        None => Ok(Source::Text(snap.previous_content.clone())),
    }
}

fn after_source(
    snap: &FileSnapshot,
    later: Option<&FileSnapshot>,
    root: Option<&Path>,
    notes: &mut Vec<String>,
) -> Result<Source, String> {
    if snap.action == "delete" {
        return Ok(Source::Missing);
    }
    if !snap.is_binary {
        return Ok(Source::Text(snap.new_content.clone()));
    }
    if let Some(name) = &snap.after_blob {
        return fs_ops::load_blob(name).map(Source::Bytes);
    }
    // 예전 버전 기록: 바뀐 뒤 내용이 없으므로 다음 변경 직전 상태나 현재 파일로 대신한다.
    if let Some(next) = later {
        notes.push("이 기록에는 바뀐 뒤 내용이 없어, 다음 변경 직전의 파일과 비교했습니다.".to_string());
        return before_source(next);
    }
    if let Some(root) = root {
        notes.push("이 기록에는 바뀐 뒤 내용이 없어, 현재 파일과 비교했습니다.".to_string());
        let path = fs_ops::resolve_safe_path(root, &snap.relative_path)?;
        return Ok(if path.is_file() {
            Source::Bytes(std::fs::read(&path).map_err(|e| e.to_string())?)
        } else {
            Source::Missing
        });
    }
    Err("바뀐 뒤 내용을 찾을 수 없습니다.".to_string())
}

/// 원본을 비교용 텍스트로 바꾼다. (텍스트, 문서 변환 여부, 바이너리 여부)
async fn to_text<R: Runtime>(
    app: &AppHandle<R>,
    path: &str,
    src: Source,
    notes: &mut Vec<String>,
) -> (Option<String>, bool, bool) {
    let text = match src {
        Source::Missing => return (None, false, false),
        Source::Text(t) => t,
        Source::Bytes(bytes) => {
            if let Some(kind) = DocKind::from_path(path) {
                match doc_bridge::extract_text(app, kind, &bytes).await {
                    Ok(md) => return (Some(clip(md, notes)), true, false),
                    Err(e) => {
                        notes.push(format!("문서 내용을 읽지 못했습니다: {}", e));
                        return (None, true, true);
                    }
                }
            }
            match String::from_utf8(bytes) {
                Ok(t) => t,
                Err(_) => return (None, false, true),
            }
        }
    };
    (Some(clip(text, notes)), false, false)
}

fn clip(text: String, notes: &mut Vec<String>) -> String {
    if text.chars().count() <= MAX_DIFF_CHARS {
        return text;
    }
    let note = format!("내용이 길어 앞 {}자만 비교합니다.", MAX_DIFF_CHARS);
    if !notes.contains(&note) {
        notes.push(note);
    }
    text.chars().take(MAX_DIFF_CHARS).collect()
}

pub async fn build_snapshot_diff<R: Runtime>(
    app: &AppHandle<R>,
    snap: &FileSnapshot,
    later: Option<&FileSnapshot>,
    root: Option<&Path>,
) -> Result<SnapshotDiff, String> {
    let mut notes = Vec::new();
    let before_src = before_source(snap)?;
    let after_src = after_source(snap, later, root, &mut notes)?;
    let before_exists = !matches!(before_src, Source::Missing);
    let after_exists = !matches!(after_src, Source::Missing);

    let (before, doc_b, bin_b) = to_text(app, &snap.relative_path, before_src, &mut notes).await;
    let (after, doc_a, bin_a) = to_text(app, &snap.relative_path, after_src, &mut notes).await;

    let kind = if bin_b || bin_a {
        "binary"
    } else if doc_b || doc_a || (DocKind::from_path(&snap.relative_path).is_some() && snap.is_binary) {
        "document"
    } else {
        "text"
    };
    if kind == "binary" && !notes.iter().any(|n| n.starts_with("문서")) {
        notes.push("텍스트로 볼 수 없는 파일이라 내용을 비교할 수 없습니다.".to_string());
    }
    if kind == "document" {
        notes.push("문서의 글자 내용만 Markdown으로 바꿔 비교합니다 (서식·이미지 변경은 표시되지 않음).".to_string());
    }

    Ok(SnapshotDiff {
        snapshot_id: snap.id.clone(),
        relative_path: snap.relative_path.clone(),
        action: snap.action.clone(),
        created_at: snap.created_at.clone(),
        before,
        after,
        before_exists,
        after_exists,
        kind: kind.to_string(),
        notes,
        superseded: later.is_some(),
        source: snap.source.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    #[test]
    fn text_write_modify_delete() {
        let app = tauri::test::mock_app();
        let dir = tempdir().unwrap();
        let root = dir.path();

        let created = fs_ops::write_file_with_snapshot(root, "p", "a.md", "하나\n둘\n").unwrap();
        let d = block_on(build_snapshot_diff(app.handle(), &created, None, Some(root))).unwrap();
        assert_eq!((d.before_exists, d.after_exists), (false, true));
        assert_eq!(d.before, None);
        assert_eq!(d.after.as_deref(), Some("하나\n둘\n"));
        assert_eq!(d.kind, "text");

        let modified = fs_ops::modify_file_with_snapshot(root, "p", "a.md", "둘", "셋").unwrap();
        let d = block_on(build_snapshot_diff(app.handle(), &modified, None, Some(root))).unwrap();
        assert_eq!(d.before.as_deref(), Some("하나\n둘\n"));
        assert_eq!(d.after.as_deref(), Some("하나\n셋\n"));

        let deleted = fs_ops::delete_path_with_snapshot(root, "p", "a.md").unwrap();
        let d = block_on(build_snapshot_diff(app.handle(), &deleted, None, Some(root))).unwrap();
        assert_eq!((d.before_exists, d.after_exists), (true, false));
        assert_eq!(d.before.as_deref(), Some("하나\n셋\n"));
        assert!(d.notes.is_empty());
    }

    #[test]
    fn binary_without_after_blob_uses_next_change_then_disk() {
        let app = tauri::test::mock_app();
        let dir = tempdir().unwrap();
        let root = dir.path();
        let v1: Vec<u8> = vec![0xff, 0x00, 0x01];
        let v2: Vec<u8> = vec![0xff, 0x00, 0x02];
        std::fs::write(root.join("x.bin"), &v1).unwrap();
        let mut first = fs_ops::write_bytes_with_snapshot(root, "p", "x.bin", &v2, "write").unwrap();
        assert!(first.after_blob.is_some());
        // 예전 버전 기록처럼 바뀐 뒤 백업을 지운다.
        first.after_blob = None;
        let second = fs_ops::write_bytes_with_snapshot(root, "p", "./X.bin", &v1, "write").unwrap();
        let all = vec![first.clone(), second.clone()];
        assert_eq!(next_change_of_same_file(&all, &first).map(|s| s.id.clone()), Some(second.id.clone()));
        assert!(next_change_of_same_file(&all, &second).is_none());

        let d = block_on(build_snapshot_diff(app.handle(), &first, next_change_of_same_file(&all, &first), Some(root)))
            .unwrap();
        assert_eq!(d.kind, "binary");
        assert!(d.notes.iter().any(|n| n.contains("다음 변경 직전")));
        assert!(d.superseded);

        let d = block_on(build_snapshot_diff(app.handle(), &first, None, Some(root))).unwrap();
        assert!(d.notes.iter().any(|n| n.contains("현재 파일")));
        assert!(d.after_exists);
    }

    #[test]
    fn long_text_is_clipped() {
        let app = tauri::test::mock_app();
        let dir = tempdir().unwrap();
        let big = "가".repeat(MAX_DIFF_CHARS + 10);
        let snap = fs_ops::write_file_with_snapshot(dir.path(), "p", "big.txt", &big).unwrap();
        let d = block_on(build_snapshot_diff(app.handle(), &snap, None, Some(dir.path()))).unwrap();
        assert_eq!(d.after.unwrap().chars().count(), MAX_DIFF_CHARS);
        assert!(d.notes.iter().any(|n| n.contains("앞")));
    }
}
