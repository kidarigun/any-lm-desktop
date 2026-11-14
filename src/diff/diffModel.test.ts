import { describe, expect, it } from "vitest";
import { collapseContext, computeDiff, toSplitRows } from "./diffModel";

const text = (segs: { text: string }[]) => segs.map((s) => s.text).join("");

describe("변경 비교", () => {
  it("추가·삭제 줄 수와 줄 번호를 센다", () => {
    const d = computeDiff("하나\n둘\n셋\n", "하나\n둘셋\n셋\n넷\n");
    expect(d.added).toBe(2);
    expect(d.removed).toBe(1);
    expect(d.rows.map((r) => [r.kind, r.oldNo, r.newNo, text(r.segments)])).toEqual([
      ["ctx", 1, 1, "하나"],
      ["del", 2, null, "둘"],
      ["add", null, 2, "둘셋"],
      ["ctx", 3, 3, "셋"],
      ["add", null, 4, "넷"],
    ]);
  });

  it("짝지은 줄은 바뀐 단어만 강조한다", () => {
    const d = computeDiff("연간 매출은 120억 원이다.\n", "연간 매출은 135억 원이다.\n");
    const del = d.rows.find((r) => r.kind === "del")!;
    const add = d.rows.find((r) => r.kind === "add")!;
    // 숫자만 바뀌었으므로 숫자만 강조된다 ("억"과 나머지 문장은 그대로).
    expect(del.segments.filter((s) => s.changed).map((s) => s.text).join("")).toBe("120");
    expect(add.segments.filter((s) => s.changed).map((s) => s.text).join("")).toBe("135");
    expect(text(add.segments)).toBe("연간 매출은 135억 원이다.");
  });

  it("거의 다 바뀐 줄은 줄 전체로 표시한다", () => {
    const d = computeDiff("alpha beta\n", "완전히 다른 문장\n");
    expect(d.rows.every((r) => r.segments.every((s) => !s.changed))).toBe(true);
  });

  it("새 파일·삭제된 파일·CRLF", () => {
    expect(computeDiff("", "a\nb\n").added).toBe(2);
    expect(computeDiff("a\nb\n", "").removed).toBe(2);
    const same = computeDiff("a\r\nb\r\n", "a\nb\n");
    expect(same.added + same.removed).toBe(0);
  });

  it("나란히 보기는 삭제·추가를 한 줄씩 짝짓는다", () => {
    const d = computeDiff("a\nb\nc\n", "a\nB\nC\nD\n");
    const split = toSplitRows(d.rows);
    expect(split.map((r) => [r.left && text(r.left.segments), r.right && text(r.right.segments)])).toEqual([
      ["a", "a"],
      ["b", "B"],
      ["c", "C"],
      [null, "D"],
    ]);
  });

  it("바뀐 곳 주변만 남기고 접는다", () => {
    const before = Array.from({ length: 30 }, (_, i) => `줄${i + 1}`).join("\n") + "\n";
    const after = before.replace("줄15\n", "줄15 수정\n");
    const d = computeDiff(before, after);
    const c = collapseContext(d.rows, (r) => r.kind === "ctx", 3);
    const gaps = c.filter((x) => x.type === "gap");
    expect(gaps).toHaveLength(2);
    expect(gaps.map((g) => (g.type === "gap" ? g.items.length : 0))).toEqual([11, 12]);
    const shown = c.filter((x) => x.type === "item");
    expect(shown).toHaveLength(3 + 2 + 3);
  });

  it("짧은 변경 없는 구간은 접지 않는다", () => {
    const d = computeDiff("a\nb\nc\nd\ne\nf\ng\nh\ni\n", "A\nb\nc\nd\ne\nf\ng\nh\nI\n");
    const c = collapseContext(d.rows, (r) => r.kind === "ctx", 3);
    expect(c.every((x) => x.type === "item")).toBe(true);
  });
});
