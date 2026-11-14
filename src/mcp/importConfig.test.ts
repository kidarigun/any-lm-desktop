import { describe, expect, it } from "vitest";
import { joinArgs, parseMcpJson, splitArgs } from "./importConfig";

describe("MCP 설정 가져오기", () => {
  it("Claude Desktop 형식을 읽고 비밀 값은 따로 둔다", () => {
    const servers = parseMcpJson(`{
      "mcpServers": {
        "filesystem": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "C:\\\\Users\\\\me\\\\Desktop"] },
        "github": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"], "env": { "GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_x" } },
        "remote": { "type": "http", "url": "https://mcp.example.com/mcp", "headers": { "Authorization": "Bearer t" }, "disabled": true }
      }
    }`);
    expect(servers.map((s) => s.config.name)).toEqual(["filesystem", "github", "remote"]);
    expect(servers[0].config.args[2]).toBe("C:\\Users\\me\\Desktop");
    expect(servers[1].config.envKeys).toEqual(["GITHUB_PERSONAL_ACCESS_TOKEN"]);
    expect(servers[1].env.GITHUB_PERSONAL_ACCESS_TOKEN).toBe("ghp_x");
    expect(servers[2].config).toMatchObject({ transport: "http", url: "https://mcp.example.com/mcp", headerKeys: ["Authorization"], enabled: false });
    expect(servers[2].headers.Authorization).toBe("Bearer t");
  });

  it("VS Code 형식, 이름 묶음 조각, 서버 하나도 읽는다", () => {
    expect(parseMcpJson(`{"servers": {"a": {"type": "stdio", "command": "uvx", "args": ["mcp-server-time"]}}}`)[0].config.command).toBe("uvx");
    expect(parseMcpJson(`"b": {"url": "http://localhost:3000/mcp"},`)[0].config).toMatchObject({ name: "b", transport: "http" });
    expect(parseMcpJson(`{"command": "node", "args": ["server.js"]}`)[0].config.name).toBe("MCP 서버");
  });

  it("잘못된 내용은 알기 쉬운 오류를 낸다", () => {
    expect(() => parseMcpJson("{ not json")).toThrow(/JSON 형식/);
    expect(() => parseMcpJson(`{"mcpServers": {}}`)).toThrow(/찾지 못했습니다/);
  });

  it("인자 줄을 나누고 다시 합친다", () => {
    const args = splitArgs(`-y @scope/pkg "C:\\My Files" '' --flag=1`);
    expect(args).toEqual(["-y", "@scope/pkg", "C:\\My Files", "", "--flag=1"]);
    expect(splitArgs(joinArgs(args))).toEqual(args);
  });
});
