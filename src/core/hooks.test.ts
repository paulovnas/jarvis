import { expect, it } from "vitest";
import { hookCatalogSchema, hookSchema } from "./hooks";

const hook = { id: "a".repeat(32), name: "Formatação", event: "PostToolUse", command: "bun run format", matcher: "write|edit", timeoutSeconds: 30, enabled: true };

it("accepts editable hooks and read-only native handlers without synthetic commands", () => {
  expect(hookCatalogSchema.parse({ revision: 2, hooks: [hook], nativeHooks: [{ id: "native-policy", name: "Política", event: "BeforeAgent", description: "Prepara o agente", command: null, matcher: null, timeoutSeconds: null }] }).nativeHooks[0].command).toBeNull();
  expect(hookSchema.parse({ ...hook, command: "  cat <<EOF\nhello\nEOF\n" }).command).toBe("  cat <<EOF\nhello\nEOF\n");
});

it("rejects native IDs, unsupported events, oversized UTF-8 and invalid timeouts", () => {
  for (const patch of [{ id: "native-policy" }, { id: "A".repeat(32) }, { event: "UnknownEvent" }, { timeoutSeconds: 0 }, { timeoutSeconds: 601 }, { name: "é".repeat(81) }, { command: " " }]) expect(hookSchema.safeParse({ ...hook, ...patch }).success).toBe(false);
});

it.each(["SubagentStart", "SubagentStop", "Interrupt", "SessionEnd"])("accepts the Codex lifecycle event %s", event => {
  expect(hookSchema.parse({ ...hook, event }).event).toBe(event);
});
