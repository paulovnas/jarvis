import { expect, it } from "vitest";
import { backupModelMappingSchema } from "./settings-backup";

it("retains the optional secondary model in backup mappings", () => {
  const choice = { account: "work", model: "primary", reasoning: null, fallback: { executor: "jarvis", account: "personal", model: "secondary", reasoning: "high" } };
  const mapping = { targetId: "builtin:standard/builder", choice };
  expect(backupModelMappingSchema.parse(mapping)).toEqual(mapping);
  expect(backupModelMappingSchema.parse({ ...mapping, choice: { ...choice, fallback: null } }).choice.fallback).toBeNull();
});
