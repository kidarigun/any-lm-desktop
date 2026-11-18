import { describe, expect, it } from "vitest";
import { docKindOf, imageMimeType, isBinaryKind, isPreviewOnlyKind, isPythonScript, opensInExternalApp } from "./formats";

describe("파일 여는 방법", () => {
  it("xlsx·pptx·pdf·이미지는 캔버스에서 미리보기만 한다", () => {
    expect(docKindOf("결과/견적.XLSX")).toBe("xlsx");
    expect(isBinaryKind("xlsx")).toBe(true);
    for (const k of ["xlsx", "pptx", "pdf", "image"] as const) expect(isPreviewOnlyKind(k)).toBe(true);
    for (const k of ["markdown", "docx", "hwpx", "html", "text"] as const) expect(isPreviewOnlyKind(k)).toBe(false);
    expect(docKindOf("공문/시행문.HWPX")).toBe("hwpx");
    expect(isBinaryKind("hwpx")).toBe(true);
  });

  it("이미지 파일 형식을 알아보고 캔버스에서 연다", () => {
    for (const f of ["chart.png", "photo.JPG", "anim.gif", "banner.webp", "icon.SVG", "favicon.ico", "raw.bmp"]) {
      expect(docKindOf(f)).toBe("image");
      expect(opensInExternalApp(f)).toBe(false);
    }
    expect(isBinaryKind("image")).toBe(true);
    expect(imageMimeType("chart.PNG")).toBe("image/png");
    expect(imageMimeType("photo.jpg")).toBe("image/jpeg");
    expect(imageMimeType("icon.svg")).toBe("image/svg+xml");
    expect(imageMimeType("banner.webp")).toBe("image/webp");
  });

  it("옛 엑셀·문서·미디어만 기본 앱으로, 문서·이미지·텍스트는 캔버스로 연다", () => {
    for (const f of ["a.XLS", "x.hwp", "clip.mp4"]) expect(opensInExternalApp(f)).toBe(true);
    for (const f of ["out/보고서.xlsx", "chart.png", "photo.jpg", "a.md", "b.docx", "c.pptx", "d.pdf", "e.csv", "make.py", "noext", "공문.HWPX"]) expect(opensInExternalApp(f)).toBe(false);
  });

  it("Python 스크립트를 알아본다", () => {
    expect(isPythonScript("scripts/make_excel.py")).toBe(true);
    expect(isPythonScript("gui.PYW")).toBe(true);
    expect(isPythonScript("notes.python.txt")).toBe(false);
  });
});
