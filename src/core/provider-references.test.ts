import { expect, it } from "vitest";
import { compatibleModels, defaultChoice, modelProblem, resolveChatModel } from "./provider-references";
import { referenceAccount } from "@/test/provider-reference-fixtures";
import { customAccountFixture } from "@/test/custom-provider-fixtures";

it("explains missing, disabled and invalid model choices without a fallback", () => {
  const account = referenceAccount(); const choice = { account: account.alias, model: "gpt-test", reasoning: "high" };
  expect(modelProblem(choice, [])).toContain("não existe mais");
  expect(modelProblem(choice, [{ ...account, enabled: false }])).toContain("desativado");
  expect(modelProblem(choice, [{ ...account, modelsAvailable: false }])).toContain("indisponíveis");
  expect(modelProblem({ ...choice, model: "missing" }, [account])).toContain("não está disponível");
  expect(modelProblem({ ...choice, reasoning: "impossible" }, [account])).toContain("raciocínio");
  expect(modelProblem(choice, [account])).toBeNull();
  expect(modelProblem(choice, [{ ...account, disabledModels: [choice.model] }])).toContain("não está disponível");
});

it("offers destinations compatible with the selected tool", () => {
  const custom = customAccountFixture();
  expect(compatibleModels(custom, "web_search")).toEqual([]);
  expect(compatibleModels(custom, "vision").map(model => model.id)).toEqual(custom.custom!.models.filter(model => model.supportsImages).map(model => model.id));
  expect(compatibleModels(referenceAccount(), "image_generation")).toEqual([]);
  expect(compatibleModels({ ...referenceAccount(), providerKind: "antigravity" }, "image_generation")[0].id).toBe("gemini-3.1-flash-image");
});

it("offers only Go vision models confirmed by the catalog, independent of their name", () => {
  const go = { ...referenceAccount(), alias: "opencode-go-pessoal", providerKind: "opencode-go", models: [
    { id: "qwen-vl", name: "Qwen Vision", reasoningLevels: [], defaultReasoningLevel: null },
    { id: "claude-text", name: "Text", reasoningLevels: [], defaultReasoningLevel: null },
  ], visionModels: ["qwen-vl"] };
  expect(compatibleModels(go, "vision").map(model => model.id)).toEqual(["qwen-vl"]);
  expect(compatibleModels({ ...go, visionModels: undefined }, "vision")).toEqual([]);
  expect(compatibleModels({ ...go, visionModels: [] }, "vision")).toEqual([]);
  expect(compatibleModels({ ...go, disabledModels: ["qwen-vl"] }, "vision")).toEqual([]);
  expect(compatibleModels(go, "web_search")).toEqual([]);
});

it("waits for a fresh catalog before declaring models invalid while keeping explicit restrictions", () => {
  const account = { ...referenceAccount(), modelsAvailable: false, modelsStale: true, models: [] };
  const choice = { account: account.alias, model: "gpt-test", reasoning: "high" };
  expect(modelProblem(choice, [account])).toBeNull();
  expect(modelProblem(choice, [{ ...account, enabled: false }])).toContain("desativado");
  expect(modelProblem(choice, [{ ...account, disabledModels: [choice.model] }])).not.toBeNull();
  expect(modelProblem(choice, [{ ...account, modelsStale: false }])).toContain("indisponíveis");
});

it("reports unavailable and identical secondary targets while preserving their selection", () => {
  const account = referenceAccount();
  const choice = { account: account.alias, model: "gpt-test", reasoning: "high" };
  expect(modelProblem({ ...choice, fallback: { ...choice, reasoning: null } }, [account])).toContain("diferente do principal");
  expect(modelProblem({ ...choice, fallback: { ...choice, account: "removed" } }, [account])).toContain("Modelo secundário: O provedor removed");
  expect(modelProblem({ ...choice, fallback: { ...choice, account: "secondary" } }, [account, { ...account, alias: "secondary" }])).toBeNull();
});

it("applies only the explicit replacement for that conversation and original choice", () => {
  const source = { account: "old", model: "old-model", reasoning: null };
  const target = { account: "new", model: "new-model", reasoning: "high" };
  const bindings = [{ itemKey: "chat:c1", source, target }];
  expect(resolveChatModel(bindings, "c1", source)).toEqual(target);
  expect(resolveChatModel(bindings, "c2", source)).toEqual(source);
  expect(resolveChatModel(bindings, "c1", { ...source, model: "manual" }).model).toBe("manual");
});

it("uses a real effort for new choices while accepting saved Ultra configurations", () => {
  const account = referenceAccount();
  account.models[0] = { ...account.models[0], reasoningLevels: ["low", "max", "ultra"], defaultReasoningLevel: "ultra" };
  expect(defaultChoice(account, account.models[0]).reasoning).toBe("max");
  expect(modelProblem({ account: account.alias, model: account.models[0].id, reasoning: "ultra" }, [account])).toBeNull();
});

it("keeps explicit chat agent replacements scoped and stronger than legacy chat bindings", () => {
  const source = { account: "old", model: "primary", reasoning: null };
  const legacy = { account: "legacy", model: "other", reasoning: null };
  const target = { account: "new", model: "replacement", reasoning: null };
  const bindings = [
    { itemKey: "chat:c1", source, target: legacy },
    { itemKey: "chat:c1:agent:standard/builder", source, target },
  ];
  expect(resolveChatModel(bindings, "c1", source, "standard/builder")).toEqual(target);
  expect(resolveChatModel(bindings, "c1", source, "designer/designer")).toEqual(legacy);
  expect(resolveChatModel(bindings, "c2", source, "standard/builder")).toEqual(source);
  const edited = { ...source, model: "explicit" };
  expect(resolveChatModel(bindings, "c1", edited, "standard/builder")).toEqual(edited);
});
