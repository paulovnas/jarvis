import { describe, expect, it } from "vitest";
import { turnOptionsSchema } from "./chat";
import { modelChoiceSchema } from "./workflow-catalog";
import { modelProblem, resolveChatModel } from "./provider-references";
import { claudeModels, claudeRuntimeSchema, executionChoice, executionSelection, executorOf, selectModelChoice } from "./executors";

describe("execution choices", () => {
  it.each(["primary", "secondary"] as const)("swaps complete assignments when %s selects the opposite model", slot => {
    const primary = { executor: "jarvis" as const, account: "work", model: "model", reasoning: "high" };
    const secondary = { executor: "claude" as const, account: "", model: "sonnet", reasoning: "max" };
    const next = slot === "primary" ? secondary : primary;
    const swapped = selectModelChoice({ ...primary, fallback: secondary }, { ...next, reasoning: null }, slot);
    expect(swapped).toEqual({ ...secondary, fallback: primary });
    expect(modelChoiceSchema.safeParse(swapped).success).toBe(true);
  });
  it("keeps provider identities distinct and retains None when there is nothing to swap", () => {
    const primary = { account: "work", model: "model", reasoning: "high" };
    const current = { ...primary, fallback: null };
    expect(selectModelChoice(current, primary, "secondary")).toEqual(current);
    const secondary = { ...primary, account: "personal" };
    expect(selectModelChoice(current, secondary, "secondary")).toEqual({ ...primary, fallback: secondary });
    expect(selectModelChoice(null, primary, "primary")).toEqual(primary);
    expect(selectModelChoice({ ...primary, fallback: secondary }, { ...primary, reasoning: "low" }, "primary")).toEqual({ ...primary, reasoning: "low", fallback: secondary });
  });
  it("keeps legacy choices on Jarvis and round-trips provider model paths", () => {
    const legacy = { account: "work", model: "vendor/model", reasoning: "high" };
    expect(executorOf(modelChoiceSchema.parse(legacy))).toBe("jarvis");
    expect(executorOf(turnOptionsSchema.parse({ ...legacy, mode: "build", approvalMode: "manual" }))).toBe("jarvis");
    expect(executionChoice(executionSelection(legacy)!)).toEqual({ ...legacy, executor: "jarvis" });
  });
  it("preserves Claude identity without inventing a provider or applying provider remaps", () => {
    const choice = { executor: "claude" as const, account: "", model: "sonnet", reasoning: "high" };
    expect(executionChoice(executionSelection(modelChoiceSchema.parse(choice))!)).toEqual(choice);
    expect(modelProblem(choice, [])).toBeNull();
    expect(resolveChatModel([{ itemKey: "chat:c1", source: { ...choice, executor: "jarvis" }, target: { account: "other", model: "other", reasoning: null } }], "c1", choice)).toEqual(choice);
  });
  it("uses the runtime catalog and only its advertised effort levels", () => {
    const runtime = claudeRuntimeSchema.parse({ installed: true, authenticated: true, version: "1", error: null, models: [{ id: "runtime-model", name: "Modelo do CLI", description: "", reasoningLevels: ["low", "high"], defaultReasoning: "high" }] });
    expect(claudeModels(runtime)).toEqual([{ value: "runtime-model", label: "Modelo do CLI", reasoningLevels: ["low", "high"], defaultReasoningLevel: "high" }]);
    expect(claudeModels(null)).toEqual([]);
    expect(claudeModels({ ...runtime, preferences: { enabled: true, disabledModels: ["runtime-model"] } })).toEqual([]);
    expect(claudeModels({ ...runtime, preferences: { enabled: false, disabledModels: [] } })).toEqual([]);
    expect(claudeModels({ ...runtime, preferences: { enabled: true, disabledModels: ["removed-model"] } })).toHaveLength(1);
  });
});
