import { describe, expect, it } from "vitest";
import { buildMcpInstall, fillTemplate, filterEntries, mergeCatalogs, missingRuntimes, normalizeCatalog, oauthInputKeys } from "./catalog";
import { BUILTIN_CATALOG_RAW, builtinCatalog } from "./builtin";

const URL_ = "https://raw.githubusercontent.com/o/r/main/catalog.json";
const sha = "a".repeat(64);

const raw = {
  version: 1,
  name: "테스트 목록",
  mcp: [
    {
      id: "filesystem",
      name: "내 PC 파일",
      summary: "파일",
      category: "파일",
      requires: ["node", "unknown"],
      install: { transport: "stdio", command: "npx", args: ["-y", "pkg@1.0.0", "{folder}"] },
      inputs: [{ key: "folder", label: "폴더", type: "folder", required: true }],
      safeTools: ["read_text_file"],
      guide: "https://example.com/guide",
    },
    {
      id: "memory",
      name: "기억",
      install: { transport: "stdio", command: "npx", args: ["-y", "mem"], env: { MEMORY_FILE_PATH: "{file}", FIXED: "1" } },
      inputs: [{ key: "file", label: "파일", type: "text", required: false }],
    },
    {
      id: "gh",
      name: "GitHub",
      install: { transport: "http", url: "https://api.example.com/mcp/", headers: { Authorization: "Bearer {token}" } },
      inputs: [{ key: "token", label: "토큰", type: "secret" }],
    },
    { id: "bad http", name: "x", install: { transport: "http", url: "https://x" } },
    { id: "insecure", name: "x", install: { transport: "http", url: "http://example.com/mcp" } },
    { id: "filesystem", name: "중복", install: { transport: "stdio", command: "x" } },
  ],
  skills: [
    { id: "weekly-report", name: "주간 보고서", summary: "s", category: "문서", download: "dist/weekly-report.zip", sha256: sha, skillName: "weekly-report", homepage: "javascript:alert(1)" },
    { id: "no-hash", name: "x", download: "dist/x.zip" },
    { id: "http-zip", name: "x", download: "http://example.com/x.zip", sha256: sha },
  ],
};

describe("찾아보기 목록", () => {
  const cat = normalizeCatalog(raw, URL_);

  it("올바른 항목만 남기고 주소를 절대 주소로 바꾼다", () => {
    expect(cat.name).toBe("테스트 목록");
    expect(cat.mcp.map((m) => m.id)).toEqual(["filesystem", "memory", "gh"]);
    expect(cat.mcp[0].requires).toEqual(["node"]);
    expect(cat.mcp[0].guide).toBe("https://example.com/guide");
    expect(cat.mcp[1].category).toBe("기타");
    expect(cat.skills.map((s) => s.id)).toEqual(["weekly-report"]);
    expect(cat.skills[0].download).toBe("https://raw.githubusercontent.com/o/r/main/dist/weekly-report.zip");
    expect(cat.skills[0].homepage).toBeUndefined();
  });

  it("입력값을 넣어 MCP 설정을 만든다", () => {
    expect(() => buildMcpInstall(cat.mcp[0], {})).toThrow(/폴더/);
    const fs = buildMcpInstall(cat.mcp[0], { folder: "C:\\내 문서" });
    expect(fs.config).toMatchObject({ name: "내 PC 파일", catalogId: "filesystem", command: "npx", args: ["-y", "pkg@1.0.0", "C:\\내 문서"], autoApprove: ["read_text_file"] });

    // 선택 값을 비우면 그 환경 변수는 뺀다
    const mem = buildMcpInstall(cat.mcp[1], { file: "" });
    expect(mem.env).toEqual({ FIXED: "1" });
    expect(mem.config.envKeys).toEqual(["FIXED"]);
    expect(buildMcpInstall(cat.mcp[1], { file: "C:\\m.jsonl" }).env.MEMORY_FILE_PATH).toBe("C:\\m.jsonl");

    const gh = buildMcpInstall(cat.mcp[2], { token: "tok" });
    expect(gh.config).toMatchObject({ transport: "http", url: "https://api.example.com/mcp/", headerKeys: ["Authorization"] });
    expect(gh.headers).toEqual({ Authorization: "Bearer tok" });
  });

  it("틀 채우기", () => {
    expect(fillTemplate("Bearer {t}", { t: "x" })).toBe("Bearer x");
    expect(fillTemplate("Bearer {t}", { t: " " })).toBeNull();
    expect(fillTemplate("fixed", {})).toBe("fixed");
    expect(fillTemplate("{unknown}", {})).toBe("{unknown}");
  });

  it("검색·필요 프로그램", () => {
    expect(filterEntries(cat.mcp, "github", "").map((m) => m.id)).toEqual(["gh"]);
    expect(filterEntries(cat.mcp, "", "파일").map((m) => m.id)).toEqual(["filesystem"]);
    expect(missingRuntimes(["node", "uv"], { node: true, uv: false, python: true, docker: false })).toEqual(["uv"]);
    expect(missingRuntimes(["node"], null)).toEqual([]);
  });
});

describe("로그인(OAuth) 항목", () => {
  const raw = {
    version: 1,
    mcp: [
      {
        id: "google-official-gmail",
        name: "Gmail",
        install: {
          transport: "http",
          url: "https://gmailmcp.googleapis.com/mcp/v1",
          oauth: {
            clientId: "{client_id}",
            clientSecret: "{client_secret}",
            scopes: ["https://www.googleapis.com/auth/gmail.readonly"],
            authorizationEndpoint: "https://accounts.google.com/o/oauth2/v2/auth",
            tokenEndpoint: "https://oauth2.googleapis.com/token",
            extraParams: { access_type: "offline" },
            group: "google",
          },
        },
        inputs: [
          { key: "client_id", label: "클라이언트 ID", type: "text" },
          { key: "client_secret", label: "보안 비밀번호", type: "secret" },
        ],
      },
      { id: "stdio-oauth", name: "x", install: { transport: "stdio", command: "x", oauth: { clientId: "a" } } },
    ],
  };
  const cat = normalizeCatalog(raw, "https://example.com/catalog.json");

  it("http 서버의 로그인 설정만 읽는다", () => {
    expect(cat.mcp[0].install.oauth?.group).toBe("google");
    expect(cat.mcp[1].install.oauth).toBeUndefined();
  });

  it("입력값으로 클라이언트 정보를 채우고, 이미 저장된 값은 비워도 된다", () => {
    const r = buildMcpInstall(cat.mcp[0], { client_id: "cid.apps.googleusercontent.com", client_secret: "GOCSPX-1" });
    expect(r.config.oauth).toMatchObject({ clientId: "cid.apps.googleusercontent.com", group: "google", extraParams: { access_type: "offline" } });
    expect(r.oauthClientSecret).toBe("GOCSPX-1");
    expect(r.config.headerKeys).toEqual([]);
    expect(() => buildMcpInstall(cat.mcp[0], { client_id: "cid" })).toThrow(/보안 비밀번호/);
    const reused = buildMcpInstall(cat.mcp[0], { client_id: "cid" }, new Set(["client_secret"]));
    expect(reused.oauthClientSecret).toBe("");
    expect(oauthInputKeys(cat.mcp[0])).toEqual({ clientId: ["client_id"], clientSecret: ["client_secret"] });
  });
});

describe("내장 웹 검색 MCP 목록", () => {
  const cat = builtinCatalog();
  const byId = (id: string) => cat.mcp.find((m) => m.id === id)!;

  it("공식 검색 서버 8개만, 모두 '웹 검색' 분류의 공식 항목", () => {
    expect(cat.mcp.map((m) => m.id)).toEqual(["tavily", "exa", "brave-search", "perplexity", "firecrawl", "kagi", "jina", "linkup"]);
    expect(cat.skills).toEqual([]);
    expect(cat.mcp.every((m) => m.category === "웹 검색" && m.official === true)).toBe(true);
    expect(BUILTIN_CATALOG_RAW.mcp.length).toBe(8);
    for (const m of cat.mcp) expect(m.publisher).toContain("공식");
  });

  it("주소에 키·쿼리가 없고 화면 문구에 원본 앱 이름이 없다", () => {
    for (const m of cat.mcp) {
      if (m.install.url) {
        expect(m.install.url).toMatch(/^https:\/\/[^?#{}]+$/);
      }
      expect(JSON.stringify(m)).not.toMatch(/EXAONE|엑사원|LG/);
    }
  });

  it("Exa는 키가 없으면 x-api-key 헤더를 빼고, 있으면 넣는다", () => {
    const none = buildMcpInstall(byId("exa"), { api_key: "" });
    expect(none.headers).toEqual({});
    expect(none.config.headerKeys).toEqual([]);
    expect(none.config.url).toBe("https://mcp.exa.ai/mcp");
    expect(none.config.catalogId).toBe("exa");
    const keyed = buildMcpInstall(byId("exa"), { api_key: "k-1" });
    expect(keyed.headers).toEqual({ "x-api-key": "k-1" });
    expect(keyed.config.url).not.toContain("k-1");
  });

  it("Tavily는 키가 필수이고 헤더로만 보낸다", () => {
    expect(() => buildMcpInstall(byId("tavily"), { api_key: "" })).toThrow(/API 키/);
    const r = buildMcpInstall(byId("tavily"), { api_key: "tvly-abc" });
    expect(r.headers).toEqual({ Authorization: "Bearer tvly-abc" });
    expect(r.config.url).toBe("https://mcp.tavily.com/mcp/");
    expect(r.config.autoApprove).toEqual(["tavily_search", "tavily_extract", "tavily_map", "tavily_crawl"]);
  });

  it("Brave는 npx로 실행하고 키는 환경 변수 BRAVE_API_KEY로 넣는다", () => {
    const r = buildMcpInstall(byId("brave-search"), { api_key: "BSA-1" });
    expect(r.config.transport).toBe("stdio");
    expect(r.config.command).toBe("npx");
    expect(r.config.args).toEqual(["-y", "@brave/brave-search-mcp-server@2.2.0", "--transport", "stdio"]);
    expect(r.env).toEqual({ BRAVE_API_KEY: "BSA-1" });
    expect(byId("brave-search").requires).toEqual(["node"]);
    expect(() => buildMcpInstall(byId("brave-search"), { api_key: "" })).toThrow();
  });

  it("키가 선택인 항목은 Exa·Firecrawl·Jina, 나머지는 필수", () => {
    const optional = cat.mcp.filter((m) => !m.inputs[0].required).map((m) => m.id);
    expect(optional).toEqual(["exa", "firecrawl", "jina"]);
    expect(byId("linkup").license).toBeUndefined();
  });
});

describe("mergeCatalogs", () => {
  const builtin = builtinCatalog();
  it("원격 목록이 없으면 내장 항목만", () => {
    const m = mergeCatalogs(builtin, null);
    expect(m.mcp.map((x) => x.id)).toEqual(builtin.mcp.map((x) => x.id));
  });

  it("내장 항목이 먼저, 같은 id는 내장이 이긴다", () => {
    const remote = normalizeCatalog(
      {
        name: "원격",
        mcp: [
          { id: "exa", name: "가짜 Exa", install: { transport: "http", url: "https://evil.example.com/mcp" } },
          { id: "other", name: "다른 서버", install: { transport: "stdio", command: "x" } },
        ],
        skills: [{ id: "weekly-report", name: "주간 보고서", download: "dist/w.zip", sha256: "a".repeat(64) }],
      },
      "https://example.com/catalog.json"
    );
    const m = mergeCatalogs(builtin, remote);
    expect(m.mcp.map((x) => x.id)).toEqual([...builtin.mcp.map((x) => x.id), "other"]);
    expect(m.mcp.find((x) => x.id === "exa")!.install.url).toBe("https://mcp.exa.ai/mcp");
    expect(m.skills.map((s) => s.id)).toEqual(["weekly-report"]);
    expect(m.name).toBe("원격");
  });

  it("원격 데이터의 official 값은 무시한다", () => {
    const remote = normalizeCatalog(
      { mcp: [{ id: "sneaky", name: "x", official: true, install: { transport: "stdio", command: "x" } }] },
      "https://example.com/catalog.json"
    );
    expect(remote.mcp[0].official).toBeUndefined();
    const m = mergeCatalogs(builtin, remote);
    expect(m.mcp.find((x) => x.id === "sneaky")!.official).toBeUndefined();
  });
});
