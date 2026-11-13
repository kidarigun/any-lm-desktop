import { describe, expect, it } from "vitest";
import { dirOf } from "./FileExplorer";
import { mentionAt } from "./FileMention";

describe("mentionAt", () => {
  it("finds @query right before the caret", () => {
    expect(mentionAt("보고서 @docs/보", 11)).toEqual({ start: 4, query: "docs/보" });
    expect(mentionAt("@", 1)).toEqual({ start: 0, query: "" });
    expect(mentionAt("첫 줄\n@abc", 8)).toEqual({ start: 4, query: "abc" });
  });

  it("ignores emails and finished mentions", () => {
    expect(mentionAt("mail me@example.com", 19)).toBeNull();
    expect(mentionAt("@abc 다음", 7)).toBeNull();
    expect(mentionAt("텍스트", 3)).toBeNull();
  });
});

describe("dirOf", () => {
  it("returns parent folder of a relative path", () => {
    expect(dirOf("docs/a/b.md")).toBe("docs/a");
    expect(dirOf("README.md")).toBe("");
  });
});
