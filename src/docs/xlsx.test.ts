import { readFileSync, writeFileSync } from "node:fs";
import JSZip from "jszip";
import { describe, expect, it } from "vitest";
import { base64ToBytes, bytesToBase64 } from "./base64";
import { handleDocBridgeRequest } from "./convert";
import { columnIndex, columnName, markdownToSheets, markdownToXlsx, parseCell, readXlsxSheets, replaceTextInXlsx, xlsxToMarkdown } from "./xlsx";

const SALES = `# 2026 매출 보고서

## 월별 매출
| 월 | 매출 | 비용 | 이익 | 이익률 |
|---|---:|---:|---:|---:|
| 1월 | 1,200 | 800 | =B2-C2 | =D2/B2 |
| 2월 | 1,350 | 900 | =B3-C3 | =D3/B3 |
| **합계** | =SUM(B2:B3) | =SUM(C2:C3) | =SUM(D2:D3) | |

## 메모
| 날짜 | 내용 | 비율 |
|---|---|---|
| 2026-01-31 | 결산 a\\|b | 12.5% |
| 2026-02-28 | 코드 007 | 3% |
`;

async function sheetXml(bytes: Uint8Array, n: number) {
  const zip = await JSZip.loadAsync(bytes);
  return zip.file(`xl/worksheets/sheet${n}.xml`)!.async("string");
}

describe("엑셀 칸 값", () => {
  it("숫자·백분율·날짜·수식을 알아본다", () => {
    expect(parseCell("1,200")).toEqual({ t: "n", v: 1200, style: "thousands" });
    expect(parseCell("2026")).toEqual({ t: "n", v: 2026, style: "general" });
    expect(parseCell("-3.5")).toEqual({ t: "n", v: -3.5, style: "general" });
    expect(parseCell("12.5%")).toEqual({ t: "n", v: 0.125, style: "percent1" });
    expect(parseCell("2026-01-31")).toEqual({ t: "n", v: 46053, style: "date" });
    expect(parseCell("=SUM(B2:B13)")).toEqual({ t: "f", f: "SUM(B2:B13)" });
    expect(parseCell("=A1*B1")).toEqual({ t: "f", f: "A1*B1" });
  });

  it("코드·전화번호·잘못된 날짜·단위가 붙은 값은 글자로 둔다", () => {
    for (const s of ["007", "010-1234-5678", "2026-02-30", "1,200원", "12,34", "1234567890123456789"]) {
      expect(parseCell(s)?.t, s).toBe("s");
    }
    expect(parseCell("**합계**")).toEqual({ t: "s", v: "합계", bold: true });
    expect(parseCell("file_name_v2")).toEqual({ t: "s", v: "file_name_v2", bold: false });
    expect(parseCell("  ")).toBeNull();
  });

  it("열 이름 변환", () => {
    expect([0, 25, 26, 701, 702].map(columnName)).toEqual(["A", "Z", "AA", "ZZ", "AAA"]);
    expect(["A", "Z", "AA", "ZZ", "AAA"].map(columnIndex)).toEqual([0, 25, 26, 701, 702]);
  });
});

describe("Markdown → 시트", () => {
  it("제목마다 시트, 머리글이 1행, 빈 제목 시트는 버린다", () => {
    const sheets = markdownToSheets(SALES);
    expect(sheets.map((s) => s.name)).toEqual(["월별 매출", "메모"]);
    expect(sheets[0].rows[0]).toEqual(["월", "매출", "비용", "이익", "이익률"]);
    expect(sheets[0].rows[1][3]).toBe("=B2-C2");
    expect([...sheets[0].headerRows]).toEqual([0]);
    expect(sheets[1].rows[1][1]).toBe("결산 a|b");
  });

  it("제목 없는 표, csv 블록, 여러 표, 겹치는 시트 이름", () => {
    const sheets = markdownToSheets(
      "| a | b |\n|---|---|\n| 1 | 2 |\n\n```csv\n이름,값\n\"홍, 길동\",3\n```\n\n## 같은:이름\n본문 줄\n## 같은?이름\n| x |\n|---|\n| 1 |",
      "Sheet1"
    );
    expect(sheets.map((s) => s.name)).toEqual(["Sheet1", "같은 이름", "같은 이름 (2)"]);
    expect(sheets[0].rows).toEqual([["a", "b"], ["1", "2"], [], ["이름", "값"], ["홍, 길동", "3"]]);
    expect([...sheets[0].headerRows]).toEqual([0, 3]);
    expect(sheets[1].rows).toEqual([["본문 줄"]]);
  });
});

describe("xlsx 만들기·읽기·고치기", () => {
  it("만든 파일을 다시 읽으면 값과 수식이 그대로다", async () => {
    const bytes = await markdownToXlsx(SALES, "매출");
    const md = await xlsxToMarkdown(bytes);
    expect(md).toContain("## 월별 매출");
    expect(md).toContain("| 월 | 매출 | 비용 | 이익 | 이익률 |");
    expect(md).toContain("| 1월 | 1200 | 800 | =B2-C2 | =D2/B2 |");
    expect(md).toContain("| 합계 | =SUM(B2:B3) | =SUM(C2:C3) | =SUM(D2:D3) |  |");
    expect(md).toContain("| 2026-01-31 | 결산 a\\|b | 0.125 |");
    expect(md).toContain("코드 007");

    const s1 = await sheetXml(bytes, 1);
    expect(s1).toContain('state="frozen"'); // 머리글 고정
    expect(s1).toContain('<c r="B2" s="2"><v>1200</v></c>'); // 천 단위 서식
    const zip = await JSZip.loadAsync(bytes);
    expect(await zip.file("xl/workbook.xml")!.async("string")).toContain('fullCalcOnLoad="1"');
  });

  it("서식을 유지한 채 글자·숫자·수식을 고친다", async () => {
    let bytes = await markdownToXlsx(SALES);
    bytes = await replaceTextInXlsx(bytes, "1,350", "1,500");
    bytes = await replaceTextInXlsx(bytes, "결산", "연말 결산");
    bytes = await replaceTextInXlsx(bytes, "=D2/B2", "=ROUND(D2/B2,3)");
    const md = await xlsxToMarkdown(bytes);
    expect(md).toContain("| 2월 | 1500 | 900 |");
    expect(md).toContain("연말 결산 a\\|b");
    expect(md).toContain("=ROUND(D2/B2,3)");
    expect(await sheetXml(bytes, 1)).toContain('<c r="B3" s="2"><v>1500</v></c>');
    await expect(replaceTextInXlsx(bytes, "없는 문구", "x")).rejects.toThrow("찾지 못했습니다");
  });

  it("다른 형식처럼 처음 찾은 한 칸만 바꾼다", async () => {
    let bytes = await markdownToXlsx("| 지점 | 비고 |\n|---|---|\n| 서울 | 서울 본점 |\n| 서울 | $1 |\n");
    bytes = await replaceTextInXlsx(bytes, "서울", "부산");
    bytes = await replaceTextInXlsx(bytes, "$1", "$&x");
    const md = await xlsxToMarkdown(bytes);
    expect(md).toContain("| 부산 | 서울 본점 |");
    expect(md).toContain("| 서울 | $&x |");
  });

  it("문서 변환 다리로 만들고 읽고 고친다", async () => {
    const gen = await handleDocBridgeRequest({ requestId: "1", op: "generate", kind: "xlsx", markdown: SALES, title: "t" });
    expect(gen.ok).toBe(true);
    const rep = await handleDocBridgeRequest({
      requestId: "2", op: "replace", kind: "xlsx", dataBase64: gen.dataBase64, target: "800", replacement: "850",
    });
    expect(rep.ok).toBe(true);
    const ext = await handleDocBridgeRequest({ requestId: "3", op: "extract", kind: "xlsx", dataBase64: rep.dataBase64 });
    expect(ext.text).toContain("| 1월 | 1200 | 850 |");
  });

  /**
   * 다른 프로그램과 주고받기 확인 (ANYLM_XLSX_DIR를 지정했을 때만):
   * - ours.xlsx를 써 두면 openpyxl 같은 외부 프로그램으로 열어 볼 수 있다.
   * - external.xlsx(외부 프로그램이 만든 파일, 공유 문자열 사용)가 있으면 읽어 external.md로 남긴다.
   */
  it.runIf(!!process.env.ANYLM_XLSX_DIR)("외부 프로그램과 주고받기", async () => {
    const dir = process.env.ANYLM_XLSX_DIR!;
    writeFileSync(`${dir}/ours.xlsx`, await markdownToXlsx(SALES, "매출"));
    let external: Uint8Array | null = null;
    try {
      external = new Uint8Array(readFileSync(`${dir}/external.xlsx`));
    } catch {
      external = null;
    }
    if (external) {
      const md = await xlsxToMarkdown(external);
      writeFileSync(`${dir}/external.md`, md);
      const edited = await replaceTextInXlsx(external, "서울", "부산");
      writeFileSync(`${dir}/external_edited.xlsx`, edited);
    }
    expect(bytesToBase64(base64ToBytes("AA=="))).toBe("AA==");
  });
});

describe("엑셀 미리보기용 읽기", () => {
  it("엑셀에 보이는 대로 숫자 서식을 살리고, 수식은 따로 알려 준다", async () => {
    const bytes = await markdownToXlsx(`## 견적
| 품목 | 금액 | 비율 | 날짜 | 합계 |
|---|---|---|---|---|
| 서버 | 1,200 | 12.5% | 2026-01-31 | =B2*2 |
`);
    const [sheet] = await readXlsxSheets(bytes);
    expect(sheet.name).toBe("견적");
    expect(sheet.width).toBe(5);
    const row = sheet.rows[1];
    expect(row[0]).toMatchObject({ display: "서버", numeric: false });
    expect(row[1]).toMatchObject({ display: "1,200", raw: "1200", numeric: true });
    expect(row[2]).toMatchObject({ display: "12.5%", raw: "0.125", numeric: true });
    expect(row[3]).toMatchObject({ display: "2026-01-31", numeric: false });
    expect(row[4]).toMatchObject({ display: "=B2*2", formula: "B2*2" });
    // AI가 읽는 Markdown은 예전처럼 원래 값과 수식을 쓴다.
    expect(await xlsxToMarkdown(bytes)).toContain("| 서버 | 1200 | 0.125 | 2026-01-31 | =B2*2 |");
  });

  it("행·열 한도를 넘으면 잘렸다고 알린다", async () => {
    const rows = Array.from({ length: 30 }, (_, i) => `| ${i} | ${i * 2} |`).join("\n");
    const bytes = await markdownToXlsx(`| a | b |
|---|---|
${rows}
`);
    const [sheet] = await readXlsxSheets(bytes, { maxRows: 10, maxCols: 1 });
    expect(sheet.rows.length).toBe(10);
    expect(sheet.width).toBe(1);
    expect(sheet.truncated).toBe(true);
  });
});
