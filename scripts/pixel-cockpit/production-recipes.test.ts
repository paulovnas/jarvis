import { describe, expect, it } from "vitest";
import { enhanceMotionRecipe, productionRecipe, productionRecipes } from "./production-recipes";
import { MOTION_RECIPES, compileMotionRecipe } from "../../src-tauri/resources/pixel-cockpit/upstream/src/lib/motionRecipes";

describe("pixel-art production recipes", () => {
  it("keeps the original motion, QA and user parameters while adding production discipline", () => {
    const compiled = {
      prompt: "IDENTITY: Approved source\nPHASES: Four frames",
      notes: "User-approved tempo",
      sections: [{ id: "identity", lines: ["Approved source"] }, { id: "phases", lines: ["Four frames"] }],
      metadata: { compilerVersion: "original", frameCount: 4, modifiers: { tempo: "slow" } },
      qaContract: { fps: 7 },
    };
    const before = JSON.stringify(compiled);
    const result = enhanceMotionRecipe(compiled, "walk-cycle", "biped");
    expect(result.prompt).toContain("alternate");
    expect(result.prompt).toContain("native pixel grid");
    expect(result.prompt).toContain("Four frames");
    expect(result.metadata).toEqual(compiled.metadata);
    expect(result.qaContract).toEqual({ fps: 7 });
    expect(result.notes).toContain("User-approved tempo");
    expect(JSON.stringify(compiled)).toBe(before);
    expect(enhanceMotionRecipe(result, "walk-cycle", "biped").prompt).toBe(result.prompt);
  });

  it("preserves jump height, asymmetry and non-biped anatomy", () => {
    expect(productionRecipe("jump-hop").guidance).toMatch(/apex|vertical/i);
    expect(productionRecipe("directional-anchor", "northwest").guidance).toContain("northwest");
    expect(productionRecipe("directional-anchor", "northwest").guidance).toMatch(/mirror|handedness/i);
    const compiled = { prompt: "", notes: "", sections: [{ id: "identity", lines: [] as string[] }] };
    const result = enhanceMotionRecipe(compiled, "walk-cycle", "quadruped");
    expect(result.prompt).toContain("quadruped");
    expect(result.prompt).not.toContain("left and right feet");
  });

  it("provides actionable phases for every exposed recipe without fixing model or frame count", () => {
    for (const { id, title } of productionRecipes) {
      const recipe = productionRecipe(id, "south");
      expect(title).toBeTruthy();
      expect(recipe.guidance.length).toBeGreaterThan(100);
      expect(recipe.guidance).not.toMatch(/fal\.ai|Seedance|exactly (8|10) frames|256.?256/);
    }
    expect(() => productionRecipe("unknown")).toThrow("Receita");
    expect(() => productionRecipe("directional-anchor", "")).toThrow("direção");
  });

  it("retains the actual upstream recipe phases, topology and quality contracts", () => {
    for (const recipe of MOTION_RECIPES) {
      const original=compileMotionRecipe({recipe,directions:["side"],topology:recipe.defaultTopology});
      const result=enhanceMotionRecipe(original,recipe.id,recipe.defaultTopology);
      expect(result.qaContract).toEqual(original.qaContract);
      expect(result.metadata).toEqual(original.metadata);
      expect(result.negativePrompt).toBe(original.negativePrompt);
      expect(result.sections.filter((section) => section.id!=="identity")).toEqual(original.sections.filter((section) => section.id!=="identity"));
      expect(result.prompt).toContain("native pixel grid");
    }
  });
});
