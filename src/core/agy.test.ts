import { expect, it } from "vitest";
import { agyModels, agyRuntimeSchema, DEFAULT_AGY_PREFERENCES } from "./agy";
import { executionChoice, executionLabel, executionSelection, selectModelChoice } from "./executors";
import { modelChoiceSchema } from "./workflow-catalog";
import { modelProblem } from "./provider-references";
import { backupModelMappingSchema } from "./settings-backup";
import { systemSnapshotSchema } from "./system-preferences";

it("keeps AGY optional and exposes only enabled runtime models and advertised efforts", () => {
  const runtime = agyRuntimeSchema.parse({ installed: true, authenticated: true, version: "1.2.13", error: null, models: [{ id: "gemini", name: "Gemini", description: "", reasoningLevels: ["low", "medium", "high", "max"], defaultReasoning: null }] });
  expect(DEFAULT_AGY_PREFERENCES).toEqual({ enabled: false, showUsage: true, disabledModels: [] });
  expect(agyModels(null)).toEqual([]);
  expect(agyModels(runtime)).toEqual([]);
  expect(agyModels({ ...runtime, preferences: { ...DEFAULT_AGY_PREFERENCES, enabled: true } })).toEqual([{ value: "gemini", label: "Gemini", reasoningLevels: ["low", "medium", "high", "max"], defaultReasoningLevel: null }]);
  expect(agyModels({ ...runtime, preferences: { enabled: true, disabledModels: ["gemini"] } })).toEqual([]);
});

it("round-trips AGY primary and secondary models through profiles, backup and selections", () => {
  const agy = { executor: "agy" as const, account: "", model: "gemini", reasoning: "high" };
  const claude = { executor: "claude" as const, account: "", model: "sonnet", reasoning: "max" };
  const choice = { ...agy, fallback: claude };
  expect(executionChoice(executionSelection(agy)!)).toEqual(agy);
  expect(executionLabel(agy)).toBe("Antigravity CLI / gemini");
  expect(modelChoiceSchema.parse(choice)).toEqual(choice);
  expect(backupModelMappingSchema.parse({ targetId: "builtin:planned/designer", choice }).choice).toEqual(choice);
  expect(modelProblem(choice, [])).toBeNull();
  expect(selectModelChoice(choice, claude, "primary")).toEqual({ ...claude, fallback: agy });
  expect(selectModelChoice({ ...claude, fallback: agy }, claude, "secondary")).toEqual(choice);
  expect(systemSnapshotSchema.parse({ preferences: { preventSleep: "off", notifications: false, askUserTimeoutSeconds: 30, agy: DEFAULT_AGY_PREFERENCES }, sleepInhibited: false, sleepError: null, notificationError: null }).preferences.agy).toEqual(DEFAULT_AGY_PREFERENCES);
});
