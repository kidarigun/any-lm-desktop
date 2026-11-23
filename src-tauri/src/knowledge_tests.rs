//! knowledge.rs 단위 테스트 (웹뷰 없이: 추출 단계는 클로저로 바꿔 끼운다)

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn idx_with(files: Vec<(&str, &str, Vec<&str>)>) -> KnowledgeIndex {
    KnowledgeIndex {
        version: INDEX_VERSION,
        files: files
            .into_iter()
            .map(|(folder, path, chunks)| IndexedFile {
                folder: folder.into(),
                path: path.into(),
                size: 1,
                mtime: 1,
                chunks: chunks.into_iter().map(String::from).collect(),
            })
            .collect(),
        ..Default::default()
    }
}

fn hit(path: &str, text: &str, score: f64) -> Hit {
    Hit { path: path.into(), chunk_index: 0, text: text.into(), score }
}

// ---------------------------------------------------------------- 토큰

#[test]
fn tokenizer_lowercases_ascii_words_and_numbers() {
    assert_eq!(tokenize("Hello, World 2049! AX-17_b"), vec!["hello", "world", "2049", "ax", "17", "b"]);
}

#[test]
fn tokenizer_makes_hangul_bigrams_and_single_syllable_unigrams() {
    assert_eq!(tokenize("출시일은"), vec!["출시", "시일", "일은"]);
    assert_eq!(tokenize("이 문서"), vec!["이", "문서"]);
    // 한글과 영문이 붙어 있어도 따로 나눈다
    assert_eq!(tokenize("ZK-7코드"), vec!["zk", "7", "코드"]);
    assert!(tokenize("   ...  ").is_empty());
}

// ---------------------------------------------------------------- 조각

#[test]
fn chunker_counts_chars_and_overlaps() {
    let text = "가".repeat(3_000);
    let chunks = chunk_text(&text);
    let lens: Vec<usize> = chunks.iter().map(|c| c.chars().count()).collect();
    assert_eq!(lens, vec![1_200, 1_200, 1_000], "바이트가 아니라 글자 수로 센다");
    let tail: String = chunks[0].chars().skip(1_200 - CHUNK_OVERLAP).collect();
    assert!(chunks[1].starts_with(&tail), "다음 조각은 앞 조각의 끝 200자로 시작한다");
}

#[test]
fn chunker_prefers_paragraph_then_line_boundaries() {
    let p = |c: char| c.to_string().repeat(500);
    let text = format!("{}\n\n{}\n\n{}\n\n{}", p('a'), p('b'), p('c'), p('d'));
    let chunks = chunk_text(&text);
    assert!(chunks[0].ends_with("\n\n"), "문단 경계에서 끊는다: {:?}", &chunks[0][chunks[0].len() - 5..]);
    assert_eq!(chunks[0].chars().count(), 1_004);
    // 문단 경계가 없으면 줄 경계
    let text = format!("{}\n{}\n{}", p('a'), p('b'), p('c'));
    let chunks = chunk_text(&text);
    assert!(chunks[0].ends_with('\n') && chunks[0].chars().count() == 1_002, "{}", chunks[0].chars().count());
    // 짧은 글은 한 조각, 빈 글은 없음
    assert_eq!(chunk_text("짧은 글"), vec!["짧은 글".to_string()]);
    assert!(chunk_text(" \n\n ").is_empty());
}

#[test]
fn every_char_is_covered_by_some_chunk() {
    let text: String = (0..5_000).map(|i| if i % 97 == 0 { '\n' } else { 'x' }).collect();
    let chunks = chunk_text(&text);
    assert!(chunks.iter().all(|c| c.chars().count() <= CHUNK_CHARS));
    let total: usize = chunks.iter().map(|c| c.chars().count()).sum();
    assert!(total >= 5_000);
    assert!(text.ends_with(chunks.last().unwrap().as_str()));
}

// ---------------------------------------------------------------- BM25

fn corpus() -> KnowledgeIndex {
    idx_with(vec![
        ("F", r"F\launch.md", vec!["신제품 ZK-7741 무선 측정기의 출시일은 2027년 3월 14일로 확정되었다."]),
        ("F", r"F\budget.md", vec!["올해 마케팅 예산은 4억 원이며 대부분 온라인 광고에 쓴다."]),
        ("F", r"F\notes.txt", vec![
            "The warehouse in Busan ships orders every Tuesday and Friday.",
            "Quarterly revenue grew because the new subscription plan launched in May.",
        ]),
        ("F", r"F\misc.md", vec!["회의는 매주 월요일 오전에 열린다. 회의록은 공유 폴더에 둔다."]),
    ])
}

#[test]
fn bm25_finds_the_right_chunk_for_korean_and_english_queries() {
    let idx = corpus();
    let s = SearchIndex::build(&idx);
    let hits = s.search(&idx, "ZK-7741 출시일이 언제야?", 3, &[]);
    assert_eq!(hits[0].path, r"F\launch.md", "{hits:?}");
    let hits = s.search(&idx, "마케팅 예산은 얼마?", 3, &[]);
    assert_eq!(hits[0].path, r"F\budget.md");
    let hits = s.search(&idx, "When does the warehouse ship?", 3, &[]);
    assert_eq!((hits[0].path.as_str(), hits[0].chunk_index), (r"F\notes.txt", 0));
    let hits = s.search(&idx, "subscription revenue", 3, &[]);
    assert_eq!((hits[0].path.as_str(), hits[0].chunk_index), (r"F\notes.txt", 1));
    assert!(hits.windows(2).all(|w| w[0].score >= w[1].score));
    assert!(s.search(&idx, "전혀없는낱말", 3, &[]).is_empty());
    assert!(s.search(&idx, "   ", 3, &[]).is_empty());
}

#[test]
fn bm25_uses_k1_and_b() {
    // 문서 하나에 낱말 하나: idf·tf 식을 손으로 계산한 값과 같다
    let idx = idx_with(vec![("F", "a", vec!["apple apple"]), ("F", "b", vec!["pear"])]);
    let s = SearchIndex::build(&idx);
    let hits = s.search(&idx, "apple", 5, &[]);
    let n = 2.0f64;
    let df = 1.0f64;
    let idf = (1.0 + (n - df + 0.5) / (df + 0.5)).ln();
    let (tf, dl, avg) = (2.0, 2.0, 1.5);
    let want = idf * tf * (1.2 + 1.0) / (tf + 1.2 * (1.0 - 0.75 + 0.75 * dl / avg));
    assert!((hits[0].score - want).abs() < 1e-9, "{} vs {}", hits[0].score, want);
}

#[test]
fn search_can_be_limited_to_current_folders() {
    let idx = idx_with(vec![("A", r"A\x.md", vec!["사과 이야기"]), ("B", r"B\y.md", vec!["사과 농장"])]);
    let s = SearchIndex::build(&idx);
    let hits = s.search(&idx, "사과", 5, &["B".to_string()]);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, r"B\y.md");
}

#[test]
fn auto_hits_drop_weak_matches() {
    let hits = vec![hit("a", "x", 10.0), hit("b", "y", 3.0), hit("c", "z", 2.0)];
    let kept = keep_relevant(hits);
    assert_eq!(kept.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
}

// ---------------------------------------------------------------- 예산·프롬프트

#[test]
fn budget_is_min_of_12000_and_15_percent_of_ctx_chars() {
    assert_eq!(knowledge_budget(32_768), 7_372);
    assert_eq!(knowledge_budget(262_144), 12_000);
    assert_eq!(knowledge_budget(8_192), 1_843);
}

#[test]
fn prompt_section_lists_sources_within_budget_and_max_six() {
    assert_eq!(knowledge_prompt_section(&[], 32_768), "");
    let hits: Vec<Hit> = (0..8).map(|i| hit(&format!(r"C:\kb\f{i}.md"), &"내".repeat(500), 10.0 - i as f64)).collect();
    let s = knowledge_prompt_section(&hits, 32_768);
    assert!(s.starts_with("\n\n[참고 자료]\n"), "{s}");
    assert!(s.contains("출처 경로를 밝히") && s.contains("관련 없으면 무시"));
    assert_eq!(s.matches("### 출처: ").count(), 6, "최대 6조각");
    assert!(s.contains(r"### 출처: C:\kb\f0.md"));
    // 8K에서는 예산(1,843자) 안에 3개만
    let s = knowledge_prompt_section(&hits, 8_192);
    assert_eq!(s.matches("### 출처: ").count(), 3, "{s}");
    let body: usize = s.split("### 출처: ").skip(1).map(|p| p.chars().count() + 7).sum();
    assert!(body <= knowledge_budget(8_192), "{body}");
    // 조각 하나가 예산보다 크면 잘라서라도 하나는 넣는다
    let big = vec![hit("p", &"가".repeat(5_000), 1.0)];
    let s = knowledge_prompt_section(&big, 4_096);
    assert_eq!(s.matches("### 출처: ").count(), 1);
    assert!(s.chars().count() < 1_600, "{}", s.chars().count());
}

#[test]
fn tool_output_lists_hits_with_paths_and_respects_cap() {
    let hits = vec![hit(r"C:\kb\a.md", "첫 조각", 3.25), hit(r"C:\kb\b.md", "둘째 조각", 1.5)];
    let out = search_tool_output("출시일", &hits, 10_000);
    assert!(out.starts_with("[지식 폴더 검색: \"출시일\" — 2개]"), "{out}");
    assert!(out.contains("### 출처: C:\\kb\\a.md (조각 1, 점수 3.25)\n첫 조각"), "{out}");
    assert!(out.contains("### 출처: C:\\kb\\b.md"));
    let none = search_tool_output("없음", &[], 10_000);
    assert!(none.contains("찾지 못했습니다"), "{none}");
    let many: Vec<Hit> = (0..10).map(|i| hit(&format!("p{i}"), &"가".repeat(900), 1.0)).collect();
    let out = search_tool_output("q", &many, 3_000);
    assert!(out.chars().count() <= 3_000, "{}", out.chars().count());
    assert!(out.contains("생략"), "{out}");
}

#[test]
fn top_k_defaults_to_5_and_is_capped_at_10() {
    assert_eq!(top_k_arg(&serde_json::json!({})), 5);
    assert_eq!(top_k_arg(&serde_json::json!({"top_k": 3})), 3);
    assert_eq!(top_k_arg(&serde_json::json!({"top_k": 50})), 10);
    assert_eq!(top_k_arg(&serde_json::json!({"top_k": 0})), 1);
}

// ---------------------------------------------------------------- 폴더 목록

#[test]
fn add_folder_validates_and_refuses_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("자료");
    std::fs::create_dir_all(&sub).unwrap();
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "x").unwrap();
    let mut list = Vec::new();
    let p = sub.to_string_lossy().to_string();
    let added = add_folder(&mut list, &format!("  {}{}  ", p, std::path::MAIN_SEPARATOR)).unwrap();
    assert_eq!(added, p, "앞뒤 공백과 끝 구분자를 뺀다");
    assert_eq!(list, vec![p.clone()]);
    let err = add_folder(&mut list, &p).unwrap_err();
    assert!(err.contains("이미"), "{err}");
    if cfg!(windows) {
        assert!(add_folder(&mut list, &p.to_uppercase()).is_err(), "Windows는 대소문자를 가리지 않는다");
    }
    assert!(add_folder(&mut list, &file.to_string_lossy()).unwrap_err().contains("폴더가 아닙니다"));
    assert!(add_folder(&mut list, &dir.path().join("없음").to_string_lossy()).unwrap_err().contains("찾을 수 없습니다"));
    assert!(add_folder(&mut list, "relative/dir").is_err());
    assert!(add_folder(&mut list, "  ").is_err());
    remove_folder(&mut list, &format!("{}{}", p, std::path::MAIN_SEPARATOR)).unwrap();
    assert!(list.is_empty());
    assert!(remove_folder(&mut list, &p).is_err());
}

// ---------------------------------------------------------------- 훑기(건너뛰기·한도)

fn write(p: &std::path::Path, content: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

#[test]
fn scan_skips_build_dirs_unknown_types_and_counts_limits() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("a.md"), "에이");
    write(&root.join("sub/b.txt"), "비");
    write(&root.join("sub/c.rs"), "fn main() {}");
    write(&root.join("report.docx"), "PK");
    write(&root.join("image.png"), "png");
    for d in SKIP_DIRS {
        write(&root.join(d).join("x.md"), "skip");
    }
    write(&root.join("big.txt"), &"x".repeat(2_000));
    let folder = root.to_string_lossy().to_string();
    let limits = Limits { max_files: 100, text_max_bytes: 1_000, doc_max_bytes: 1_000 };
    let scan = scan_folders_with(&[folder.clone()], &limits);
    let names: Vec<String> = scan
        .files
        .iter()
        .map(|f| std::path::Path::new(&f.path).strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"))
        .collect();
    assert_eq!(names, vec!["a.md", "report.docx", "sub/b.txt", "sub/c.rs"]);
    assert_eq!(scan.files[1].kind, SourceKind::Doc(DocKind::Docx));
    assert_eq!(scan.skipped.len(), 1);
    assert!(scan.skipped[0].path.ends_with("big.txt") && scan.skipped[0].reason.contains("너무 큼"), "{:?}", scan.skipped);

    let limits = Limits { max_files: 2, ..limits };
    let scan = scan_folders_with(&[folder.clone(), root.join("없는 폴더").to_string_lossy().to_string()], &limits);
    assert_eq!(scan.files.len(), 2);
    assert_eq!(scan.skipped.iter().filter(|s| s.reason.contains("파일 수 한도")).count(), 2);
    assert_eq!(scan.missing_folders.len(), 1);
}

// ---------------------------------------------------------------- 갱신

fn scanned(path: &str, size: u64, mtime: u64) -> ScannedFile {
    ScannedFile { folder: "F".into(), path: path.into(), size, mtime, kind: SourceKind::Text }
}

#[test]
fn plan_refresh_reextracts_only_changed_and_new_files() {
    let mut old = idx_with(vec![("F", "same", vec!["s"]), ("F", "changed", vec!["c"]), ("F", "deleted", vec!["d"])]);
    old.skipped.push(SkippedFile { folder: "F".into(), path: "broken".into(), size: 5, mtime: 5, reason: "x".into(), failed: true });
    let scan = ScanResult {
        files: vec![scanned("same", 1, 1), scanned("changed", 2, 1), scanned("new", 1, 1), scanned("broken", 5, 5)],
        ..Default::default()
    };
    let plan = plan_refresh(&old, &["F".to_string()], scan.clone(), false);
    assert_eq!(plan.keep.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), vec!["same"]);
    assert_eq!(plan.to_extract.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), vec!["changed", "new"]);
    assert_eq!(plan.keep_skipped.len(), 1, "못 읽은 파일은 바뀌지 않았으면 다시 시도하지 않는다");
    assert!(plan.stale.contains_key(&path_key("changed")));
    assert_eq!(plan.removed, 1);
    // 다시 색인(force)은 모두 다시 추출
    let plan = plan_refresh(&old, &["F".to_string()], scan, true);
    assert_eq!(plan.to_extract.len(), 4);
}

#[test]
fn plan_refresh_keeps_entries_of_missing_folders_and_drops_removed_folders() {
    let old = idx_with(vec![("F", "f1", vec!["a"]), ("G", "g1", vec!["b"]), ("H", "h1", vec!["c"])]);
    let scan = ScanResult { missing_folders: vec!["G".into()], ..Default::default() };
    let plan = plan_refresh(&old, &["F".to_string(), "G".to_string()], scan, false);
    assert_eq!(plan.keep.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), vec!["g1"]);
    assert_eq!(plan.removed, 2, "F의 지운 파일과 목록에서 뺀 H 폴더");
}

#[tokio::test]
async fn refresh_extracts_incrementally_and_defers_after_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("a.md"), "사과 이야기");
    write(&root.join("b.txt"), "배 이야기");
    let folders = vec![root.to_string_lossy().to_string()];
    let calls = AtomicUsize::new(0);
    let extract = |f: ScannedFile| {
        calls.fetch_add(1, Ordering::SeqCst);
        async move { std::fs::read_to_string(&f.path).map_err(|e| e.to_string()) }
    };
    let empty = KnowledgeIndex::default();
    let out = refresh_index(&empty, &folders, RefreshMode::Full, extract, |_, _| {}).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(out.changed && out.index.last_indexed.is_some());
    assert_eq!(out.index.files.len(), 2);

    // 바뀐 것이 없으면 다시 추출하지 않는다
    let out2 = refresh_index(&out.index, &folders, RefreshMode::Quick, extract, |_, _| {}).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(!out2.changed);

    // 하나를 고치고(크기 변경) 하나를 지우면: 고친 것만 추출, 지운 것은 빠진다
    write(&root.join("a.md"), "사과 이야기 더 길게");
    std::fs::remove_file(root.join("b.txt")).unwrap();
    let out3 = refresh_index(&out.index, &folders, RefreshMode::Quick, extract, |_, _| {}).await;
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert!(out3.changed);
    assert_eq!(out3.index.files.len(), 1);
    assert!(out3.index.files[0].chunks[0].contains("더 길게"));

    // 기한이 이미 지났으면 추출을 미루고 옛 조각을 그대로 둔다
    write(&root.join("a.md"), "사과 이야기 세 번째로 고침");
    let out4 = refresh_index_until(&out3.index, &folders, RefreshMode::Quick, Some(std::time::Instant::now()), extract, |_, _| {}).await;
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(out4.deferred, 1);
    assert!(out4.index.files[0].chunks[0].contains("더 길게"), "미룬 파일은 옛 조각 유지");

    // 추출 실패는 건너뜀으로 센다
    let failing = |_f: ScannedFile| async move { Err::<String, String>("변환 실패".into()) };
    let out5 = refresh_index(&KnowledgeIndex::default(), &folders, RefreshMode::Full, failing, |_, _| {}).await;
    assert_eq!(out5.index.files.len(), 0);
    assert_eq!(out5.index.skipped.len(), 1);
    assert!(out5.index.skipped[0].failed && out5.index.skipped[0].reason.contains("변환 실패"));
}

// ---------------------------------------------------------------- 저장 파일

#[test]
fn index_round_trips_and_bad_files_are_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    let m = KnowledgeManager::new(dir.path().join("knowledge"));
    let idx = idx_with(vec![("F", r"F\a.md", vec!["가나다", "라마바"])]);
    let path = m.index_path("proj-1");
    write_index(&path, &idx).unwrap();
    assert_eq!(read_index(&path), Some(idx.clone()));
    std::fs::write(&path, "{ 깨진 파일").unwrap();
    assert_eq!(read_index(&path), None);
    let mut old = idx.clone();
    old.version = 0;
    write_index(&path, &old).unwrap();
    assert_eq!(read_index(&path), None, "옛 형식이면 다시 만든다");
    assert!(m.index_path("../../evil").starts_with(dir.path().join("knowledge")));
}

#[test]
fn status_counts_files_chunks_and_skipped_per_folder() {
    let mut idx = idx_with(vec![("A", r"A\1", vec!["x", "y"]), ("A", r"A\2", vec!["z"]), ("B", r"B\1", vec!["w"])]);
    idx.skipped.push(SkippedFile { folder: "A".into(), path: r"A\big".into(), size: 0, mtime: 0, reason: "너무 큼".into(), failed: false });
    idx.last_indexed = Some("2026-11-23T10:00:00+09:00".into());
    idx.missing_folders = vec!["B".into()];
    let st = status_of(&idx, &["A".to_string(), "B".to_string()]);
    assert_eq!((st.files, st.chunks, st.skipped), (3, 4, 1));
    assert_eq!((st.folders[0].files, st.folders[0].chunks, st.folders[0].skipped), (2, 3, 1));
    assert!(st.folders[1].missing);
    assert_eq!(st.skipped_samples.len(), 1);
}

#[test]
fn old_project_json_without_knowledge_folders_loads() {
    let old = r#"{"id":"p","name":"n","rootPath":"C:/x","instructions":"i","createdAt":"t"}"#;
    let p: Project = serde_json::from_str(old).unwrap();
    assert!(p.knowledge_folders.is_empty());
    let json = serde_json::to_string(&p).unwrap();
    assert!(json.contains("\"knowledgeFolders\":[]"), "{json}");
    let with = r#"{"id":"p","name":"n","rootPath":"C:/x","instructions":"i","createdAt":"t","knowledgeFolders":["D:\\자료"]}"#;
    let p: Project = serde_json::from_str(with).unwrap();
    assert_eq!(p.knowledge_folders, vec![r"D:\자료".to_string()]);
}
