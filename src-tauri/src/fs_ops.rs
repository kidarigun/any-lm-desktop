use crate::store::FileSnapshot;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileNode {
    pub name: String,
    pub relative_path: String,
    pub is_dir: bool,
    pub size: u64,
    pub children: Option<Vec<FileNode>>,
}

/// Ensures a target path stays strictly inside the project root directory.
pub fn resolve_safe_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let clean_rel = relative
        .trim()
        .trim_start_matches('/')
        .trim_start_matches('\\');

    let rel_path = Path::new(clean_rel);
    for comp in rel_path.components() {
        match comp {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "보안 오류: 프로젝트 루트 외부 경로('{}')에는 접근할 수 없습니다.",
                    relative
                ));
            }
            _ => {}
        }
    }
    Ok(root.join(rel_path))
}

const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    ".next",
    ".venv",
    "__pycache__",
];

pub fn list_project_tree(root: &Path, max_depth: usize) -> Result<Vec<FileNode>, String> {
    list_project_subtree(root, root, max_depth)
}

/// `start` 아래의 트리를 나열하되, `relative_path`는 항상 프로젝트 `root` 기준으로 만든다.
pub fn list_project_subtree(
    root: &Path,
    start: &Path,
    max_depth: usize,
) -> Result<Vec<FileNode>, String> {
    if !start.is_dir() {
        return Err(format!("디렉터리가 존재하지 않습니다: {}", start.display()));
    }
    build_tree_recursive(root, start, 0, max_depth)
}

/// 모델에 되돌려 줄 도구 출력이 지나치게 길면 앞부분만 남기고 잘라낸다.
pub fn truncate_for_model(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars).collect();
    format!(
        "{}\n\n[... 출력이 너무 길어 {}자 중 앞 {}자만 표시했습니다 ...]",
        head, total, max_chars
    )
}

fn build_tree_recursive(
    root: &Path,
    current: &Path,
    depth: usize,
    max_depth: usize,
) -> Result<Vec<FileNode>, String> {
    if depth > max_depth {
        return Ok(Vec::new());
    }
    let mut nodes = Vec::new();
    let entries = fs::read_dir(current).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if IGNORED_DIRS.contains(&name.as_str()) {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        let metadata = entry.metadata().ok();
        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);

        let children = if is_dir && depth < max_depth {
            Some(build_tree_recursive(root, &path, depth + 1, max_depth).unwrap_or_default())
        } else {
            None
        };

        nodes.push(FileNode {
            name,
            relative_path: rel,
            is_dir,
            size,
            children,
        });
    }

    nodes.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(nodes)
}

pub fn read_file_content(root: &Path, relative_path: &str) -> Result<String, String> {
    let target = resolve_safe_path(root, relative_path)?;
    if !target.exists() {
        return Err(format!("파일을 찾을 수 없습니다: {}", relative_path));
    }
    fs::read_to_string(&target).map_err(|e| format!("파일 읽기 오류 ({}): {}", relative_path, e))
}

pub fn read_file_bytes(root: &Path, relative_path: &str) -> Result<Vec<u8>, String> {
    let target = resolve_safe_path(root, relative_path)?;
    if !target.is_file() {
        return Err(format!("파일을 찾을 수 없습니다: {}", relative_path));
    }
    fs::read(&target).map_err(|e| format!("파일 읽기 오류 ({}): {}", relative_path, e))
}

// ---------------------------------------------------------------------------
// 바이너리 백업 (docx/pptx 등 텍스트가 아닌 파일의 되돌리기용)
// ---------------------------------------------------------------------------

static BLOB_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 앱 시작 시 한 번 호출해 백업 파일을 앱 데이터 폴더 아래에 두도록 한다.
pub fn set_blob_dir(dir: PathBuf) {
    let _ = fs::create_dir_all(&dir);
    let _ = BLOB_DIR.set(dir);
}

fn blob_dir() -> PathBuf {
    BLOB_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| std::env::temp_dir().join("exaone-desktop-blobs"))
}

fn save_blob(bytes: &[u8]) -> Result<String, String> {
    let dir = blob_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("백업 폴더 생성 실패: {}", e))?;
    let name = format!("{}.bin", Uuid::new_v4());
    fs::write(dir.join(&name), bytes).map_err(|e| format!("백업 파일 저장 실패: {}", e))?;
    Ok(name)
}

fn load_blob(name: &str) -> Result<Vec<u8>, String> {
    // 저장된 이름만 받는다 (경로 조작 방지).
    if name.contains(['/', '\\']) || name.contains("..") {
        return Err("잘못된 백업 파일 이름입니다.".to_string());
    }
    fs::read(blob_dir().join(name)).map_err(|e| format!("백업 파일을 읽을 수 없습니다: {}", e))
}

/// 덮어쓰기/삭제 전에 기존 파일 내용을 보존한다.
/// 텍스트면 문자열로, 바이너리면 백업 파일로 남긴다. (existed_before, previous_text, backup_blob)
fn capture_previous(target: &Path) -> Result<(bool, String, Option<String>), String> {
    if !target.is_file() {
        return Ok((target.exists(), String::new(), None));
    }
    let bytes = fs::read(target).map_err(|e| format!("기존 파일 읽기 실패: {}", e))?;
    match String::from_utf8(bytes) {
        Ok(text) => Ok((true, text, None)),
        Err(e) => Ok((true, String::new(), Some(save_blob(e.as_bytes())?))),
    }
}

pub fn write_file_with_snapshot(
    root: &Path,
    project_id: &str,
    relative_path: &str,
    content: &str,
) -> Result<FileSnapshot, String> {
    let mut snap = write_bytes_with_snapshot(root, project_id, relative_path, content.as_bytes(), "write")?;
    snap.new_content = content.to_string();
    snap.is_binary = false;
    Ok(snap)
}

/// 바이트를 그대로 파일에 쓰고 되돌리기용 스냅샷을 만든다.
pub fn write_bytes_with_snapshot(
    root: &Path,
    project_id: &str,
    relative_path: &str,
    bytes: &[u8],
    action: &str,
) -> Result<FileSnapshot, String> {
    let target = resolve_safe_path(root, relative_path)?;
    if target.is_dir() {
        return Err(format!("폴더에는 파일을 쓸 수 없습니다: {}", relative_path));
    }
    let (existed_before, previous_content, backup_blob) = capture_previous(&target)?;

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&target, bytes).map_err(|e| format!("파일 쓰기 오류 ({}): {}", relative_path, e))?;

    let text = std::str::from_utf8(bytes).ok();
    Ok(FileSnapshot {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        relative_path: relative_path.replace('\\', "/"),
        existed_before,
        previous_content,
        new_content: text.unwrap_or_default().to_string(),
        action: action.to_string(),
        created_at: chrono::Local::now().to_rfc3339(),
        backup_blob,
        is_binary: text.is_none(),
    })
}

pub fn modify_file_with_snapshot(
    root: &Path,
    project_id: &str,
    relative_path: &str,
    target_snippet: &str,
    replacement: &str,
) -> Result<FileSnapshot, String> {
    let target = resolve_safe_path(root, relative_path)?;
    if !target.exists() {
        return Err(format!("수정할 파일이 존재하지 않습니다: {}", relative_path));
    }
    let previous_content = fs::read_to_string(&target).map_err(|e| e.to_string())?;
    if !previous_content.contains(target_snippet) {
        return Err(format!(
            "파일 '{}'에서 교체할 대상 문자열(target)을 찾지 못했습니다. 정확한 원문을 확인해 주세요.",
            relative_path
        ));
    }

    let new_content = previous_content.replacen(target_snippet, replacement, 1);
    fs::write(&target, &new_content).map_err(|e| e.to_string())?;

    Ok(FileSnapshot {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        relative_path: relative_path.replace('\\', "/"),
        existed_before: true,
        previous_content,
        new_content,
        action: "modify".to_string(),
        created_at: chrono::Local::now().to_rfc3339(),
        backup_blob: None,
        is_binary: false,
    })
}

pub fn create_directory(root: &Path, relative_path: &str) -> Result<String, String> {
    let target = resolve_safe_path(root, relative_path)?;
    fs::create_dir_all(&target).map_err(|e| format!("폴더 생성 실패: {}", e))?;
    Ok(format!("폴더 생성 완료: {}", relative_path))
}

pub fn delete_path_with_snapshot(
    root: &Path,
    project_id: &str,
    relative_path: &str,
) -> Result<FileSnapshot, String> {
    let target = resolve_safe_path(root, relative_path)?;
    if !target.exists() {
        return Err(format!("삭제할 경로가 존재하지 않습니다: {}", relative_path));
    }
    let (_, previous_content, backup_blob) = capture_previous(&target)?;

    if target.is_dir() {
        fs::remove_dir_all(&target).map_err(|e| format!("폴더 삭제 실패: {}", e))?;
    } else {
        fs::remove_file(&target).map_err(|e| format!("파일 삭제 실패: {}", e))?;
    }

    Ok(FileSnapshot {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        relative_path: relative_path.replace('\\', "/"),
        existed_before: true,
        previous_content,
        new_content: String::new(),
        action: "delete".to_string(),
        created_at: chrono::Local::now().to_rfc3339(),
        backup_blob,
        is_binary: false,
    })
}

pub fn restore_snapshot(root: &Path, snapshot: &FileSnapshot) -> Result<String, String> {
    let target = resolve_safe_path(root, &snapshot.relative_path)?;
    if !snapshot.existed_before {
        if target.exists() {
            let _ = fs::remove_file(&target);
        }
        Ok(format!(
            "스냅샷 복원 완료: 새로 생성되었던 '{}' 파일을 제거했습니다.",
            snapshot.relative_path
        ))
    } else {
        if let Some(parent) = target.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match &snapshot.backup_blob {
            Some(name) => fs::write(&target, load_blob(name)?).map_err(|e| e.to_string())?,
            None => fs::write(&target, &snapshot.previous_content).map_err(|e| e.to_string())?,
        }
        Ok(format!(
            "스냅샷 복원 완료: '{}' 파일을 이전 상태로 되돌렸습니다.",
            snapshot.relative_path
        ))
    }
}

pub fn search_in_files(root: &Path, query: &str) -> Result<String, String> {
    let mut results = Vec::new();
    let q_lower = query.to_lowercase();
    search_recursive(root, root, &q_lower, &mut results, 0);
    if results.is_empty() {
        Ok(format!("'{}'에 대한 검색 결과가 없습니다.", query))
    } else {
        Ok(results.join("\n"))
    }
}

fn search_recursive(
    root: &Path,
    current: &Path,
    query_lower: &str,
    results: &mut Vec<String>,
    depth: usize,
) {
    if depth > 5 || results.len() >= 50 {
        return;
    }
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        if results.len() >= 50 {
            break;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if IGNORED_DIRS.contains(&name.as_str()) {
            continue;
        }
        if path.is_dir() {
            search_recursive(root, &path, query_lower, results, depth + 1);
        } else if path.is_file() {
            if let Ok(content) = fs::read_to_string(&path) {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                for (idx, line) in content.lines().enumerate() {
                    if line.to_lowercase().contains(query_lower) {
                        results.push(format!("{}:{}: {}", rel, idx + 1, line.trim()));
                        if results.len() >= 50 {
                            break;
                        }
                    }
                }
            }
        }
    }
}

/// 셸 명령을 비동기로 실행한다. UI 스레드와 앱 상태 잠금을 막지 않으며,
/// `timeout`을 넘기면 프로세스를 종료하고 오류를 돌려준다.
pub async fn execute_shell_command(
    root: &Path,
    command_line: &str,
    timeout: Duration,
) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    let mut cmd = {
        // 한글 출력이 깨지지 않도록 PowerShell 출력 인코딩을 UTF-8로 고정한다.
        let script = format!(
            "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $OutputEncoding = [System.Text.Encoding]::UTF8; {}",
            command_line
        );
        let mut c = tokio::process::Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
        c
    };

    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let mut c = tokio::process::Command::new("sh");
        c.args(["-c", command_line]);
        c
    };

    cmd.current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let child = cmd
        .spawn()
        .map_err(|e| format!("명령어 실행 실패: {}", e))?;

    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(res) => res.map_err(|e| format!("명령어 실행 실패: {}", e))?,
        Err(_) => {
            return Err(format!(
                "명령이 {}초 안에 끝나지 않아 중단했습니다: {}",
                timeout.as_secs(),
                command_line
            ))
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let status_code = output.status.code().unwrap_or(-1);

    Ok(format!(
        "[Exit Code: {}]\n{}{}",
        status_code,
        stdout,
        if stderr.is_empty() {
            String::new()
        } else {
            format!("\n[STDERR]\n{}", stderr)
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_read_write_modify_delete_and_undo() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // 1. Write file
        let snap_write = write_file_with_snapshot(
            root,
            "proj-1",
            "docs/report.md",
            "# 초기 보고서\n내용 A",
        )
        .unwrap();
        assert!(!snap_write.existed_before);
        let content = read_file_content(root, "docs/report.md").unwrap();
        assert_eq!(content, "# 초기 보고서\n내용 A");

        // 2. Modify file
        let snap_mod = modify_file_with_snapshot(
            root,
            "proj-1",
            "docs/report.md",
            "내용 A",
            "수정된 내용 B",
        )
        .unwrap();
        assert!(snap_mod.existed_before);
        let modified = read_file_content(root, "docs/report.md").unwrap();
        assert_eq!(modified, "# 초기 보고서\n수정된 내용 B");

        // 3. Undo modify snapshot
        restore_snapshot(root, &snap_mod).unwrap();
        let restored = read_file_content(root, "docs/report.md").unwrap();
        assert_eq!(restored, "# 초기 보고서\n내용 A");

        // 4. Delete file & undo delete
        let snap_del = delete_path_with_snapshot(root, "proj-1", "docs/report.md").unwrap();
        assert!(read_file_content(root, "docs/report.md").is_err());
        restore_snapshot(root, &snap_del).unwrap();
        assert_eq!(
            read_file_content(root, "docs/report.md").unwrap(),
            "# 초기 보고서\n내용 A"
        );

        // 5. Reject path traversal
        assert!(resolve_safe_path(root, "../outside.txt").is_err());
    }

    #[test]
    fn binary_overwrite_and_delete_can_be_undone() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let original: Vec<u8> = vec![0x50, 0x4b, 0x03, 0x04, 0xff, 0x00, 0xfe];
        std::fs::write(root.join("deck.pptx"), &original).unwrap();

        let snap = write_bytes_with_snapshot(root, "p", "deck.pptx", &[0x50, 0x4b, 0xff, 2], "write").unwrap();
        assert!(snap.backup_blob.is_some());
        assert!(snap.is_binary);
        restore_snapshot(root, &snap).unwrap();
        assert_eq!(std::fs::read(root.join("deck.pptx")).unwrap(), original);

        let del = delete_path_with_snapshot(root, "p", "deck.pptx").unwrap();
        assert!(!root.join("deck.pptx").exists());
        restore_snapshot(root, &del).unwrap();
        assert_eq!(std::fs::read(root.join("deck.pptx")).unwrap(), original);
        assert_eq!(read_file_bytes(root, "deck.pptx").unwrap(), original);
    }

    #[test]
    fn subtree_paths_are_relative_to_project_root() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        write_file_with_snapshot(root, "p", "docs/a/b.md", "x").unwrap();
        let sub = resolve_safe_path(root, "docs").unwrap();
        let nodes = list_project_subtree(root, &sub, 2).unwrap();
        assert_eq!(nodes[0].relative_path, "docs/a");
        assert_eq!(nodes[0].children.as_ref().unwrap()[0].relative_path, "docs/a/b.md");
        assert!(list_project_subtree(root, &root.join("missing"), 1).is_err());
    }

    #[test]
    fn truncate_for_model_counts_chars_not_bytes() {
        assert_eq!(truncate_for_model("가나다", 3), "가나다");
        let cut = truncate_for_model("가나다라마", 2);
        assert!(cut.starts_with("가나\n"));
        assert!(cut.contains("5자 중 앞 2자"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn shell_command_returns_utf8_output() {
        let dir = tempdir().unwrap();
        let out = execute_shell_command(dir.path(), "Write-Output '안녕 EXAONE'", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(out.contains("[Exit Code: 0]"), "{}", out);
        assert!(out.contains("안녕 EXAONE"), "{}", out);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn shell_command_times_out() {
        let dir = tempdir().unwrap();
        let err = execute_shell_command(dir.path(), "Start-Sleep -Seconds 20", Duration::from_secs(2))
            .await
            .unwrap_err();
        assert!(err.contains("2초 안에 끝나지 않아"), "{}", err);
    }
}
