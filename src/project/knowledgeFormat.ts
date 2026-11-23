import type { KnowledgeFolderStatus, KnowledgeStatus, Project } from "../types";

/** 색인 시각을 "YYYY-MM-DD HH:MM"(이 PC 시각)으로. 읽을 수 없으면 그대로. */
export function formatIndexedAt(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** 지식 폴더 카드의 상태 줄 */
export function knowledgeStatusLine(s: KnowledgeStatus): string {
  if (s.indexing) return s.total > 0 ? `색인 중… (${s.done}/${s.total})` : "색인 중…";
  if (!s.lastIndexed && s.files === 0) return "아직 색인하지 않았습니다.";
  const parts = [`파일 ${s.files}개`, `조각 ${s.chunks}개`, `건너뜀 ${s.skipped}개`];
  if (s.lastIndexed) parts.push(`마지막 색인 ${formatIndexedAt(s.lastIndexed)}`);
  if (s.deferred > 0) parts.push(`다음 질문 때 이어서 색인 ${s.deferred}개`);
  if (s.overLimit)
    parts.push(
      s.overEntryLimit
        ? "훑기 항목 수 한도(폴더·파일 200,000개)에 닿아 나머지는 색인하지 않음"
        : "파일 수 한도(3,000개)에 닿아 나머지는 색인하지 않음"
    );
  return parts.join(" · ");
}

/** 폴더 한 줄 설명 */
export function folderLine(f: KnowledgeFolderStatus | undefined): string {
  if (!f) return "색인 전";
  if (f.missing) return "폴더를 찾을 수 없습니다";
  const parts = [`파일 ${f.files}개`, `조각 ${f.chunks}개`];
  if (f.skipped > 0) parts.push(`건너뜀 ${f.skipped}개`);
  return parts.join(" · ");
}

/** 프로젝트 설정 창의 고치는 중인 값이 저장된 프로젝트와 같은지(같아야 [큰 편집기로 열기]를 쓸 수 있다). */
export function draftMatchesProject(
  draft: { id?: string; name: string; rootPath: string; instructions: string },
  saved: Project | undefined
): boolean {
  return !!saved && draft.name === saved.name && draft.rootPath === saved.rootPath && draft.instructions === saved.instructions;
}
