import { describe, expect, it } from "vitest";
import { draftMatchesProject, folderLine, formatIndexedAt, knowledgeStatusLine } from "./knowledgeFormat";
import type { KnowledgeStatus, Project } from "../types";

const status = (over: Partial<KnowledgeStatus> = {}): KnowledgeStatus => ({
  indexing: false,
  done: 0,
  total: 0,
  files: 0,
  chunks: 0,
  skipped: 0,
  deferred: 0,
  skippedSamples: [],
  lastIndexed: null,
  folders: [],
  error: null,
  overLimit: false,
  overEntryLimit: false,
  ...over,
});

describe("knowledgeFormat", () => {
  it("shows indexing progress", () => {
    expect(knowledgeStatusLine(status({ indexing: true }))).toBe("색인 중…");
    expect(knowledgeStatusLine(status({ indexing: true, done: 3, total: 10 }))).toBe("색인 중… (3/10)");
  });
  it("shows counts and the last indexed time", () => {
    const s = status({ files: 12, chunks: 40, skipped: 2, lastIndexed: "2026-11-23T09:05:00+09:00" });
    expect(knowledgeStatusLine(s)).toBe(`파일 12개 · 조각 40개 · 건너뜀 2개 · 마지막 색인 ${formatIndexedAt("2026-11-23T09:05:00+09:00")}`);
    expect(knowledgeStatusLine(status())).toBe("아직 색인하지 않았습니다.");
    expect(knowledgeStatusLine(status({ files: 1, chunks: 1, lastIndexed: "x", deferred: 4 }))).toContain("다음 질문 때 이어서 색인 4개");
    expect(knowledgeStatusLine(status({ files: 3000, chunks: 9000, lastIndexed: "x", overLimit: true }))).toContain(
      "파일 수 한도(3,000개)에 닿아 나머지는 색인하지 않음"
    );
    const entries = knowledgeStatusLine(status({ files: 10, chunks: 20, lastIndexed: "x", overLimit: true, overEntryLimit: true }));
    expect(entries).toContain("훑기 항목 수 한도(폴더·파일 200,000개)에 닿아 나머지는 색인하지 않음");
    expect(entries).not.toContain("3,000개");
  });
  it("formats the time as YYYY-MM-DD HH:MM in local time", () => {
    const d = new Date(2026, 10, 3, 7, 4);
    expect(formatIndexedAt(d.toISOString())).toBe("2026-11-03 07:04");
    expect(formatIndexedAt("엉망")).toBe("엉망");
  });
  it("describes one folder", () => {
    expect(folderLine({ path: "C:/a", files: 3, chunks: 9, skipped: 0, missing: false })).toBe("파일 3개 · 조각 9개");
    expect(folderLine({ path: "C:/a", files: 3, chunks: 9, skipped: 1, missing: false })).toBe("파일 3개 · 조각 9개 · 건너뜀 1개");
    expect(folderLine({ path: "C:/a", files: 0, chunks: 0, skipped: 0, missing: true })).toBe("폴더를 찾을 수 없습니다");
    expect(folderLine(undefined)).toBe("색인 전");
  });
  it("knows when the project modal draft equals the saved project", () => {
    const p = { id: "p", name: "n", rootPath: "C:/x", instructions: "i", createdAt: "" } as Project;
    expect(draftMatchesProject({ id: "p", name: "n", rootPath: "C:/x", instructions: "i" }, p)).toBe(true);
    expect(draftMatchesProject({ id: "p", name: "n", rootPath: "C:/x", instructions: "i2" }, p)).toBe(false);
    expect(draftMatchesProject({ id: "p", name: "n2", rootPath: "C:/x", instructions: "i" }, p)).toBe(false);
    expect(draftMatchesProject({ id: "p", name: "n", rootPath: "C:/x", instructions: "i" }, undefined)).toBe(false);
  });
});
