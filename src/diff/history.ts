import type { FileSnapshot } from "../types";

/** 같은 파일인지 비교할 때 쓰는 경로 형태 (백엔드 snapshot_diff::same_file과 같은 규칙) */
export function normalizeRelPath(p: string): string {
  return p.trim().replace(/\\/g, "/").replace(/^(\.\/)+/, "").toLowerCase();
}

/** 한 파일의 변경 기록을 최신 것부터 돌려준다. */
export function fileHistory(snapshots: FileSnapshot[], projectId: string, relativePath: string): FileSnapshot[] {
  const target = normalizeRelPath(relativePath);
  return snapshots
    .filter((s) => s.projectId === projectId && normalizeRelPath(s.relativePath) === target)
    .reverse();
}

/** 변경 기록 목록에 쓰는 짧은 시각 표시: 오늘이면 "14:05", 아니면 "11월 3일 14:05" */
export function formatWhen(iso: string, now: Date = new Date()): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const time = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  const sameDay =
    d.getFullYear() === now.getFullYear() && d.getMonth() === now.getMonth() && d.getDate() === now.getDate();
  if (sameDay) return time;
  const date = `${d.getMonth() + 1}월 ${d.getDate()}일`;
  return d.getFullYear() === now.getFullYear() ? `${date} ${time}` : `${d.getFullYear()}년 ${date} ${time}`;
}

export function sourceLabel(source: FileSnapshot["source"]): string | null {
  if (source === "agent") return "AI";
  if (source === "user") return "직접 저장";
  return null;
}
