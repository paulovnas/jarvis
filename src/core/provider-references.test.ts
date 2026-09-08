import { expect, it } from "vitest";
import { compatibleModels, modelProblem, resolveChatModel } from "./provider-references";
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
});

it("offers destinations compatible with the selected tool", () => {
  const custom = customAccountFixture();
  expect(compatibleModels(custom, "web_search")).toEqual([]);
  expect(compatibleModels(custom, "vision").map(model => model.id)).toEqual(custom.custom!.models.filter(model => model.supportsImages).map(model => model.id));
  expect(compatibleModels(referenceAccount(), "image_generation")).toEqual([]);
  expect(compatibleModels({ ...referenceAccount(), providerKind: "antigravity" }, "image_generation")[0].id).toBe("gemini-3.1-flash-image");
});

it("applies only the explicit replacement for that conversation and original choice", () => {
  const source = { account: "old", model: "old-model", reasoning: null };
  const target = { account: "new", model: "new-model", reasoning: "high" };
  const bindings = [{ itemKey: "chat:c1", source, target }];
  expect(resolveChatModel(bindings, "c1", source)).toEqual(target);
  expect(resolveChatModel(bindings, "c2", source)).toEqual(source);
  expect(resolveChatModel(bindings, "c1", { ...source, model: "manual" }).model).toBe("manual");
});
