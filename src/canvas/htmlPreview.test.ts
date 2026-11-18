import { describe, expect, it } from "vitest";
import { HTML_PREVIEW_SANDBOX, htmlPreviewRequest } from "./htmlPreview";

describe("HTML web preview", () => {
  it("keeps project previews scoped to the selected project file", () => {
    expect(
      htmlPreviewRequest(
        { filePath: "site/index.html" },
        "project-1",
        "<h1>edited</h1>"
      )
    ).toEqual({ projectId: "project-1", relativePath: "site/index.html", absolutePath: null, html: "<h1>edited</h1>" });
  });

  it("scopes explicitly opened files to their own folder", () => {
    expect(
      htmlPreviewRequest(
        { absolutePath: "C:\\demo\\page.html" },
        null,
        "<script src=\"app.js\"></script>"
      )
    ).toEqual({ projectId: null, relativePath: null, absolutePath: "C:\\demo\\page.html", html: "<script src=\"app.js\"></script>" });
  });

  it("lets the isolated preview load its own scripts and resources", () => {
    expect(HTML_PREVIEW_SANDBOX).toBe("allow-scripts allow-same-origin");
    expect(HTML_PREVIEW_SANDBOX).not.toContain("allow-top-navigation");
    expect(HTML_PREVIEW_SANDBOX).not.toContain("allow-forms");
  });
});
