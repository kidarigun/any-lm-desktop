import { describe, expect, it } from "vitest";
import { docKindOf, isBinaryKind, isPreviewOnlyKind, isPythonScript, opensInExternalApp } from "./formats";

describe("파일 여는 방법", () => {
  it("xlsx·pptx·pdf는 캔버스에서 미리보기만 한다", () => {
    expect(docKindOf("결과/견적.XLSX")).toBe("xlsx");
    expect(isBinaryKind("xlsx")).toBe(true);
    for (const k of ["xlsx", "pptx", "pdf"] as const) expect(isPreviewOnlyKind(k)).toBe(true);
    for (const k of ["markdown", "docx", "hwpx", "html", "text"] as const) expect(isPreviewOnlyKind(k)).toBe(false);
    expect(docKindOf("공문/시행문.HWPX")).toBe("hwpx");
    expect(isBinaryKind("hwpx")).toBe(true);
  });

  it("옛 엑셀·이미지는 기본 앱으로, 문서·텍스트는 캔버스로 연다", () => {
    for (const f of ["a.XLS", "chart.png", "x.hwp"]) expect(opensInExternalApp(f)).toBe(true);
    for (const f of ["out/보고서.xlsx", "a.md", "b.docx", "c.pptx", "d.pdf", "e.csv", "make.py", "noext", "공문.HWPX"]) expect(opensInExternalApp(f)).toBe(false);
  });

  it("Python 스크립트를 알아본다", () => {
    expect(isPythonScript("scripts/make_excel.py")).toBe(true);
    expect(isPythonScript("gui.PYW")).toBe(true);
    expect(isPythonScript("notes.python.txt")).toBe(false);
  });
});
