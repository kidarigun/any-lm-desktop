import { describe, expect, it } from "vitest";
import { previewDestination, previewFrameUrl } from "./devServerApi";

describe("개발 미리보기 주소", () => {
  it("로컬 주소를 정규화하고 외부 사이트는 브라우저 대상으로 표시한다", () => {
    expect(previewDestination("http://0.0.0.0:5173/a")).toEqual({ url: "http://localhost:5173/a", local: true });
    expect(previewDestination("https://test.localhost:8443/")).toEqual({ url: "https://test.localhost:8443/", local: false });
    expect(previewDestination("http://anylm-preview.localhost:4321/x")?.local).toBe(true);
    expect(previewDestination("http://[::1]:8000")?.local).toBe(true);
    expect(previewDestination("https://localhost.example.com/")?.local).toBe(false);
    expect(previewDestination("https://localhost@example.com/")).toBeNull();
    expect(previewDestination("file:///C:/secret")).toBeNull();
  });

  it("앱 자체는 프레임에 넣지 않고 IPv6 주소를 다른 호스트로 바꾸지 않는다", () => {
    expect(previewDestination("http://localhost:1420/settings", "http://localhost:1420")).toBeNull();
    expect(previewDestination("http://tauri.localhost/")).toBeNull();
    expect(previewDestination("http://tauri.localhost:5173/x")).toBeNull();
    expect(previewDestination("http://ipc.localhost:1/")).toBeNull();
    expect(previewDestination("http://asset.localhost:8080/")).toBeNull();
    expect(previewFrameUrl("http://evil.localhost:5173/")).toBeNull();
    expect(previewFrameUrl("http://[::1]:8000/")).toBeNull();
    expect(previewFrameUrl("http://localhost:8000/")).toBe("http://localhost:8000/");
  });
});
