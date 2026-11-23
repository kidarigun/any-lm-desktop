// 화면 소스에 K-EXAONE 브랜드 문구가 남지 않았는지
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const walk = (d: string): string[] =>
  readdirSync(d).flatMap((n) => (statSync(join(d, n)).isDirectory() ? walk(join(d, n)) : [join(d, n)]));

describe("branding", () => {
  it("has no K-EXAONE branding in UI sources", () => {
    const files = walk("src").filter((f) => /\.(tsx|ts)$/.test(f) && !f.endsWith(".test.ts"));
    const hits = files.filter((f) => /K-EXAONE|EXAONE|LG AI|엑사원/.test(readFileSync(f, "utf8").replace(/onExaoneStream|exaone-stream/g, "")));
    expect(hits).toEqual([]);
  });
});
