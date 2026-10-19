import { expect, it } from "vitest";
import { providerKindLabels } from "./provider-accounts";
import { compatibleModels } from "./provider-references";
import { customAccountFixture } from "@/test/custom-provider-fixtures";

it("exposes native OpenRouter while keeping manual image and vision capabilities", () => {
  expect(providerKindLabels.openrouter).toBe("OpenRouter");
  const account = { ...customAccountFixture(), providerKind: "openrouter" };
  account.custom!.baseUrl = "https://openrouter.ai/api/v1";
  account.custom!.models[0].supportsImageGeneration = true;
  expect(compatibleModels(account, "image_generation").map(model => model.id)).toEqual([account.custom!.models[0].id]);
  expect(compatibleModels(account, "vision").map(model => model.id)).toEqual(account.custom!.models.filter(model => model.supportsImages).map(model => model.id));
  account.custom!.models[0].supportsImageGeneration = false;
  expect(compatibleModels(account, "image_generation")).toEqual([]);
});
