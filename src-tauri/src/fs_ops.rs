use crate::store::FileSnapshot;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
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
    if !root.exists() {
        return Err(format!("디렉터리가 존재하지 않습니다: {}", root.display()));
    }
    build_tree_recursive(root, root, 0, max_depth)
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

pub fn write_file_with_snapshot(
    root: &Path,
    project_id: &str,
    relative_path: &str,
    content: &str,
) -> Result<FileSnapshot, String> {
    let target = resolve_safe_path(root, relative_path)?;
    let existed_before = target.exists();
    let previous_content = if existed_before {
        fs::read_to_string(&target).unwrap_or_default()
    } else {
        String::new()
    };

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&target, content).map_err(|e| format!("파일 쓰기 오류 ({}): {}", relative_path, e))?;

    Ok(FileSnapshot {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        relative_path: relative_path.replace('\\', "/"),
        existed_before,
        previous_content,
        new_content: content.to_string(),
        action: "write".to_string(),
        created_at: chrono::Local::now().to_rfc3339(),
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
    let previous_content = if target.is_file() {
        fs::read_to_string(&target).unwrap_or_default()
    } else {
        String::new()
    };

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
        fs::write(&target, &snapshot.previous_content).map_err(|e| e.to_string())?;
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

pub fn execute_shell_command(root: &Path, command_line: &str) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", command_line])
        .current_dir(root)
        .output()
        .map_err(|e| format!("명령어 실행 실패: {}", e))?;

    #[cfg(not(target_os = "windows"))]
    let output = Command::new("sh")
        .args(["-c", command_line])
        .current_dir(root)
        .output()
        .map_err(|e| format!("명령어 실행 실패: {}", e))?;

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
}
