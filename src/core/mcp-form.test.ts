import { describe, expect, it } from "vitest";
import { emptyMcpDraft, readMcpDraft, writeMcpDraft } from "./mcp-form";

describe("visual MCP configuration", () => {
  it("round trips local arguments, spaces, newlines, empty values and advanced options", () => {
    const raw = { tools: { type: "local", command: ["/path with spaces/node", "server.js", "multi\nline", ""], cwd: "/my folder", environment: { KEY: "test-secret", EMPTY: "" }, enabled: false, timeout: 1001, request_timeout: 45001 } };
    const restored = JSON.parse(writeMcpDraft(readMcpDraft(JSON.stringify(raw)), true));
    expect(restored).toEqual({ tools: { ...raw.tools, request_timeout: undefined, requestTimeout: 45001 } });
  });
  it("preserves remote authentication and does not switch the enabled preference", () => {
    const raw = { remote: { type: "remote", url: "https://example.test/mcp?q=1", headers: { Authorization: "Bearer test-only", "X-Custom": "value" }, oauth: false, enabled: false, timeout: 6000, requestTimeout: 450000 } };
    expect(JSON.parse(writeMcpDraft(readMcpDraft(JSON.stringify(raw)), true))).toEqual(raw);
  });
  it("rejects unsupported fields instead of silently dropping them when switching tabs", () => {
    expect(() => readMcpDraft('{"test":{"type":"local","command":["node"],"futureOption":true}}')).toThrow(/texto foi preservado/);
    expect(() => readMcpDraft("{")).toThrow(/JSON inválido/);
  });
  it("never silently overwrites duplicate variable names during form-to-JSON conversion", () => {
    expect(() => writeMcpDraft({ ...emptyMcpDraft(), environment: [{ key: "KEY", value: "a" }, { key: "KEY", value: "b" }] })).toThrow(/nomes repetidos/);
    expect(() => writeMcpDraft({ ...emptyMcpDraft(), type: "remote", headers: [{ key: "Authorization", value: "a" }, { key: "authorization", value: "b" }] })).toThrow(/nomes repetidos/);
  });
  it("requires a usable program or HTTP URL and bounded timeouts before saving", () => {
    const valid = { ...emptyMcpDraft(), name: "tools", command: "node" };
    expect(() => writeMcpDraft(valid, true)).not.toThrow();
    expect(() => writeMcpDraft({ ...valid, command: " " }, true)).toThrow(/programa/);
    expect(() => writeMcpDraft({ ...valid, type: "remote", url: "file:///tmp/mcp" }, true)).toThrow(/HTTP/);
    expect(() => writeMcpDraft({ ...valid, timeout: "" }, true)).toThrow(/Tempo para conectar/);
    expect(() => writeMcpDraft({ ...valid, requestTimeout: "900001" }, true)).toThrow(/Tempo por chamada/);
  });
});
