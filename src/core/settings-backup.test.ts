import { expect, it } from "vitest";
import { backupModelMappingSchema, backupModelTargetSchema } from "./settings-backup";

it("retains the optional secondary model in backup mappings", () => {
  const choice = { account: "work", model: "primary", reasoning: null, fallback: { executor: "jarvis", account: "personal", model: "secondary", reasoning: "high" } };
  const mapping = { targetId: "builtin:standard/builder", choice };
  expect(backupModelMappingSchema.parse(mapping)).toEqual(mapping);
  expect(backupModelMappingSchema.parse({ ...mapping, choice: { ...choice, fallback: null } }).choice.fallback).toBeNull();
});

it("includes the independent chat title model among import mappings", () => {
  const target = { id: "chat_title", kind: "chat_title", label: "Títulos das conversas", details: ["Modelo independente dos agentes"] };
  expect(backupModelTargetSchema.parse(target)).toEqual(target);
  const choice = { account: "personal", model: "gpt-6-luna", reasoning: null };
  expect(backupModelMappingSchema.parse({ targetId: target.id, choice }).choice).toEqual(choice);
});
