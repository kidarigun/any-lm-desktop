import { describe, expect, it } from "vitest";
import type { FileSnapshot } from "../types";
import { fileHistory, formatWhen } from "./history";

function snap(id: string, projectId: string, relativePath: string): FileSnapshot {
  return {
    id,
    projectId,
    relativePath,
    existedBefore: true,
    previousContent: "",
    newContent: "",
    action: "write",
    createdAt: "2026-11-14T05:00:00Z",
  };
}

describe("파일 변경 기록", () => {
  it("같은 프로젝트의 같은 파일만 최신순으로 모은다", () => {
    const all = [
      snap("1", "p", "docs/a.md"),
      snap("2", "p", "docs/b.md"),
      snap("3", "q", "docs/a.md"),
      snap("4", "p", "./Docs\\A.md"),
      snap("5", "p", "docs/a.md"),
    ];
    expect(fileHistory(all, "p", "docs/a.md").map((s) => s.id)).toEqual(["5", "4", "1"]);
    expect(all.map((s) => s.id)).toEqual(["1", "2", "3", "4", "5"]);
  });

  it("오늘은 시각만, 다른 날은 날짜도 표시한다", () => {
    const now = new Date(2026, 10, 14, 18, 0);
    expect(formatWhen(new Date(2026, 10, 14, 9, 5).toISOString(), now)).toBe("09:05");
    expect(formatWhen(new Date(2026, 10, 3, 14, 30).toISOString(), now)).toBe("11월 3일 14:30");
    expect(formatWhen(new Date(2025, 0, 2, 8, 0).toISOString(), now)).toBe("2025년 1월 2일 08:00");
  });
});
