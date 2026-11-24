import { describe, expect, it } from "vitest";
import { previewDestination } from "./devServerApi";

describe("개발 미리보기 주소", () => {
  it("로컬 주소를 정규화하고 외부 사이트는 브라우저 대상으로 표시한다", () => {
    expect(previewDestination("http://0.0.0.0:5173/a")).toEqual({ url: "http://localhost:5173/a", local: true });
    expect(previewDestination("https://test.localhost:8443/")?.local).toBe(true);
    expect(previewDestination("http://[::1]:8000")?.local).toBe(true);
    expect(previewDestination("https://localhost.example.com/")?.local).toBe(false);
    expect(previewDestination("https://localhost@example.com/")).toBeNull();
    expect(previewDestination("file:///C:/secret")).toBeNull();
  });
});
