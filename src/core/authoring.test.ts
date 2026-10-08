import { expect, it } from "vitest";
import { hasMcpAuthoringValues, pendingAuthoringSchema } from "./authoring";

const proposal = {
  turnId: "turn-mcp", toolId: "tool-mcp", action: "create", summary: "Adicionar documentação.",
  catalogRevision: null, agentReferences: [], target: { kind: "mcp", server: {
    name: "docs", transport: "http", command: null, args: [], url: "https://example.com/mcp",
    enabled: true, cwd: null, envKeys: [], headerKeys: ["Authorization"],
  } },
};

it("accepts a scoped project instructions proposal and refuses another file", () => {
  const target = { kind: "project_instructions", path: "AGENTS.md", before: null, after: "# Project rules\nUse existing tests." };
  expect(pendingAuthoringSchema.parse({ ...proposal, target }).target).toEqual(target);
  expect(pendingAuthoringSchema.safeParse({ ...proposal, target: { ...target, path: "../AGENTS.md" } }).success).toBe(false);
  expect(pendingAuthoringSchema.safeParse({ ...proposal, target: { ...target, after: "" } }).success).toBe(false);
});

it("accepts a credential-free MCP approval summary", () => {
  expect(pendingAuthoringSchema.parse(proposal)).toEqual(proposal);
});

it.each(["environment", "headers"])("rejects MCP approval summaries carrying %s values", field => {
  expect(pendingAuthoringSchema.safeParse({ ...proposal, target: { ...proposal.target, server: {
    ...proposal.target.server, [field]: { Authorization: "test-only-secret" },
  } } }).success).toBe(false);
});

it("requires user values even when an MCP key matches an Object prototype property", () => {
  const server = pendingAuthoringSchema.parse(proposal).target;
  if (server.kind !== "mcp") throw new Error("Missing MCP fixture");
  const config = { ...server.server, headerKeys: ["toString"] };
  expect(hasMcpAuthoringValues(config, { environment: {}, headers: {} })).toBe(false);
  expect(hasMcpAuthoringValues(config, { environment: {}, headers: { toString: "test-only-value" } })).toBe(true);
});

it("accepts a typed manual hook deletion and rejects an empty target or native event", () => {
  const hook = { id: "a".repeat(32), name: "Verificar comandos", event: "PreToolUse", command: "node hook.js", matcher: "bash", timeoutSeconds: 30, enabled: true };
  const pending = { ...proposal, action: "delete", catalogRevision: 3, target: { kind: "hook", before: hook, after: null } };
  expect(pendingAuthoringSchema.parse(pending)).toEqual(pending);
  expect(pendingAuthoringSchema.safeParse({ ...pending, target: { kind: "hook", before: null, after: null } }).success).toBe(false);
  expect(pendingAuthoringSchema.safeParse({ ...pending, target: { kind: "hook", before: { ...hook, event: "BeforeAgent" }, after: null } }).success).toBe(false);
});
