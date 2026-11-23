//! 프로젝트 메모리: 모델(`remember` 도구)과 사용자가 함께 쓰는 짧은 사실 목록.
//! 저장·검증·시스템 프롬프트 절 만들기는 모두 여기의 순수 함수로 두어 시험하기 쉽게 한다.

use crate::store::{MemoryItem, Project};

/// 항목 하나의 최대 글자 수
pub const MEMORY_ITEM_MAX_CHARS: usize = 500;
/// 프로젝트당 최대 항목 수
pub const MEMORY_MAX_ITEMS: usize = 200;
/// 프롬프트에 넣는 메모리 글자 수의 절대 상한
const PROMPT_BUDGET_CAP: usize = 6_000;
/// 프롬프트 메모리 예산 = 컨텍스트 글자 수 × 이 비율 (상한 `PROMPT_BUDGET_CAP`)
const PROMPT_BUDGET_RATIO: f64 = 0.08;

#[derive(Debug, Clone, PartialEq)]
pub enum AddOutcome {
    Added(MemoryItem),
    Duplicate,
}

fn new_id() -> String {
    format!("mem-{}", uuid::Uuid::new_v4().simple())
}

fn validate_text(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("기억할 내용이 비어 있습니다.".to_string());
    }
    let n = t.chars().count();
    if n > MEMORY_ITEM_MAX_CHARS {
        return Err(format!(
            "메모리 한 항목은 {}자까지입니다 (지금 {}자). 더 짧게 줄여서 다시 저장하세요.",
            MEMORY_ITEM_MAX_CHARS, n
        ));
    }
    Ok(t.to_string())
}

/// 항목을 더한다. 같은 문장이 이미 있으면 `Duplicate`. 한도를 넘으면 이유를 돌려준다.
pub fn add_item(items: &mut Vec<MemoryItem>, text: &str, source: &str, now: &str) -> Result<AddOutcome, String> {
    let t = validate_text(text)?;
    if items.iter().any(|m| m.text == t) {
        return Ok(AddOutcome::Duplicate);
    }
    if items.len() >= MEMORY_MAX_ITEMS {
        return Err(format!(
            "프로젝트 메모리는 {}개까지 저장할 수 있습니다. 쓰지 않는 항목을 먼저 지우세요.",
            MEMORY_MAX_ITEMS
        ));
    }
    let item = MemoryItem {
        id: new_id(),
        text: t,
        created_at: now.to_string(),
        updated_at: now.to_string(),
        source: source.to_string(),
    };
    items.push(item.clone());
    Ok(AddOutcome::Added(item))
}

/// 항목 글을 고친다.
pub fn update_item(items: &mut [MemoryItem], id: &str, text: &str, now: &str) -> Result<MemoryItem, String> {
    let t = validate_text(text)?;
    if items.iter().any(|m| m.id != id && m.text == t) {
        return Err("같은 내용의 메모리가 이미 있습니다.".to_string());
    }
    let m = items
        .iter_mut()
        .find(|m| m.id == id)
        .ok_or_else(|| "메모리 항목을 찾을 수 없습니다.".to_string())?;
    m.text = t;
    m.updated_at = now.to_string();
    Ok(m.clone())
}

pub fn delete_item(items: &mut Vec<MemoryItem>, id: &str) -> Result<(), String> {
    let before = items.len();
    items.retain(|m| m.id != id);
    if items.len() == before {
        return Err("메모리 항목을 찾을 수 없습니다.".to_string());
    }
    Ok(())
}

/// `remember` 도구 실행 결과: (상태, 출력, 저장이 바뀌었는지)
pub fn remember_outcome(items: &mut Vec<MemoryItem>, text: &str, now: &str) -> (&'static str, String, bool) {
    match add_item(items, text, "model", now) {
        Ok(AddOutcome::Added(m)) => ("completed", format!("기억했습니다: {}", m.text), true),
        Ok(AddOutcome::Duplicate) => ("completed", "이미 기억하고 있습니다.".to_string(), false),
        Err(e) => ("error", e, false),
    }
}

/// 시스템 프롬프트의 `[프로젝트 메모리]` 절. 항목이 없어도 `remember` 안내는 들어간다.
pub fn memory_prompt_section(items: &[MemoryItem], context_length: u32) -> String {
    let mut out = String::from(
        "

[프로젝트 메모리]
사용자가 기억하라고 하거나, 다음 대화에도 필요한 사실(선호, 결정, 용어, 마감)을 알게 되면 remember 도구로 짧게 저장하세요. 대화 내용 전체나 파일 내용은 저장하지 마세요.",
    );
    if items.is_empty() {
        return out;
    }
    let ctx_chars = context_length as f64 * crate::context_budget::CHARS_PER_TOKEN;
    let budget = ((ctx_chars * PROMPT_BUDGET_RATIO) as usize).min(PROMPT_BUDGET_CAP);
    // 새 항목부터 예산 안에 담고, 못 담은 오래된 항목은 뺀다.
    let mut used = 0usize;
    let mut keep_from = items.len();
    for (i, m) in items.iter().enumerate().rev() {
        let cost = m.text.chars().count() + 3; // "- " + 줄바꿈
        if used + cost > budget {
            break;
        }
        used += cost;
        keep_from = i;
    }
    out.push_str("
저장된 메모리:");
    if keep_from > 0 {
        out.push_str(&format!("
(오래된 메모리 {}개 생략)", keep_from));
    }
    for m in &items[keep_from..] {
        out.push_str("
- ");
        out.push_str(&m.text);
    }
    out
}

/// 앱 상태에 저장된 프로젝트의 메모리를 바꾼다. `remember` 도구와 명령이 함께 쓴다.
pub fn project_memory_mut<'a>(projects: &'a mut [Project], project_id: &str) -> Result<&'a mut Project, String> {
    projects
        .iter_mut()
        .find(|p| p.id == project_id)
        .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, text: &str) -> MemoryItem {
        MemoryItem {
            id: id.into(),
            text: text.into(),
            created_at: "t".into(),
            updated_at: "t".into(),
            source: "user".into(),
        }
    }

    #[test]
    fn add_trims_and_assigns_source() {
        let mut v = Vec::new();
        match add_item(&mut v, "  보고서는 존댓말로  ", "model", "now").unwrap() {
            AddOutcome::Added(m) => {
                assert_eq!(m.text, "보고서는 존댓말로");
                assert_eq!(m.source, "model");
                assert!(m.id.starts_with("mem-"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn duplicate_text_adds_nothing() {
        let mut v = vec![item("a", "마감은 금요일")];
        assert_eq!(add_item(&mut v, " 마감은 금요일 ", "model", "now").unwrap(), AddOutcome::Duplicate);
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn empty_text_is_rejected() {
        let mut v = Vec::new();
        assert!(add_item(&mut v, "   ", "user", "now").is_err());
    }

    #[test]
    fn item_over_500_chars_is_rejected_with_reason() {
        let mut v = Vec::new();
        let err = add_item(&mut v, &"가".repeat(501), "model", "now").unwrap_err();
        assert!(err.contains("500"), "{err}");
        assert!(v.is_empty());
        assert!(add_item(&mut v, &"가".repeat(500), "model", "now").is_ok());
    }

    #[test]
    fn more_than_200_items_is_rejected_with_reason() {
        let mut v: Vec<MemoryItem> = (0..200).map(|i| item(&format!("m{i}"), &format!("사실 {i}"))).collect();
        let err = add_item(&mut v, "새 사실", "model", "now").unwrap_err();
        assert!(err.contains("200"), "{err}");
        assert_eq!(v.len(), 200);
    }

    #[test]
    fn update_changes_text_and_time_and_checks_limits() {
        let mut v = vec![item("a", "하나"), item("b", "둘")];
        let m = update_item(&mut v, "a", "  바뀐 글 ", "later").unwrap();
        assert_eq!(m.text, "바뀐 글");
        assert_eq!(m.updated_at, "later");
        assert_eq!(m.created_at, "t");
        assert!(update_item(&mut v, "a", &"가".repeat(501), "x").is_err());
        assert!(update_item(&mut v, "a", "둘", "x").is_err()); // 다른 항목과 같은 글
        assert!(update_item(&mut v, "zzz", "글", "x").is_err());
    }

    #[test]
    fn delete_removes_or_errors() {
        let mut v = vec![item("a", "하나")];
        assert!(delete_item(&mut v, "zzz").is_err());
        delete_item(&mut v, "a").unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn remember_outcome_texts() {
        let mut v = Vec::new();
        let (status, out, changed) = remember_outcome(&mut v, "발표는 15분", "now");
        assert_eq!(status, "completed");
        assert!(out.contains("기억했습니다") && out.contains("발표는 15분"), "{out}");
        assert!(changed);
        let (status, out, changed) = remember_outcome(&mut v, "발표는 15분", "now");
        assert_eq!(status, "completed");
        assert_eq!(out, "이미 기억하고 있습니다.");
        assert!(!changed);
        let (status, out, changed) = remember_outcome(&mut v, &"가".repeat(600), "now");
        assert_eq!(status, "error");
        assert!(out.contains("500"), "{out}");
        assert!(!changed);
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn prompt_section_lists_oldest_first_and_has_guidance() {
        let items = vec![item("a", "첫째"), item("b", "둘째")];
        let s = memory_prompt_section(&items, 32768);
        assert!(s.starts_with("\n\n[프로젝트 메모리]"), "{s}");
        assert!(s.contains("remember"));
        assert!(s.find("- 첫째").unwrap() < s.find("- 둘째").unwrap());
        assert!(!s.contains("생략"));
    }

    #[test]
    fn prompt_section_without_items_still_guides() {
        let s = memory_prompt_section(&[], 32768);
        assert!(s.contains("remember"));
        assert!(!s.contains("\n- "));
    }

    #[test]
    fn prompt_section_drops_oldest_over_budget() {
        // ctx 4096토큰 → 6144자 × 0.08 = 491자 예산
        let items: Vec<MemoryItem> = (0..10).map(|i| item(&format!("m{i}"), &format!("{i}{}", "가".repeat(99)))).collect();
        let s = memory_prompt_section(&items, 4096);
        assert!(s.contains("(오래된 메모리 "), "{s}");
        assert!(s.contains("개 생략)"));
        // 가장 새 항목은 남고 가장 오래된 항목은 빠진다
        assert!(s.contains(&format!("- 9{}", "가".repeat(99))));
        assert!(!s.contains(&format!("- 0{}", "가".repeat(99))));
        let omitted: usize = s
            .split("(오래된 메모리 ")
            .nth(1)
            .and_then(|r| r.split('개').next())
            .and_then(|n| n.parse().ok())
            .unwrap();
        let kept = s.matches("\n- ").count();
        assert_eq!(omitted + kept, 10);
    }

    #[test]
    fn prompt_budget_is_capped_at_6000_chars() {
        let items: Vec<MemoryItem> = (0..60).map(|i| item(&format!("m{i}"), &format!("{i}{}", "나".repeat(499)))).collect();
        let s = memory_prompt_section(&items, 1_000_000);
        let body: usize = s.lines().filter(|l| l.starts_with("- ")).map(|l| l.chars().count()).sum();
        assert!(body <= 6_000, "{body}");
        assert!(s.contains("생략"));
    }

    #[test]
    fn old_project_json_without_memory_loads() {
        let old = r#"{"id":"p","name":"n","rootPath":"C:/x","instructions":"i","createdAt":"t"}"#;
        let p: Project = serde_json::from_str(old).unwrap();
        assert!(p.memory.is_empty());
        let with = r#"{"id":"p","name":"n","rootPath":"C:/x","instructions":"i","createdAt":"t","memory":[{"id":"m","text":"x","createdAt":"a","updatedAt":"b","source":"model"}]}"#;
        let p: Project = serde_json::from_str(with).unwrap();
        assert_eq!(p.memory[0].source, "model");
    }
}
