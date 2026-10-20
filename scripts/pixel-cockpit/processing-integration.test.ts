import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { adoptProductionCompiler, adoptProcessingPanel } from "./processing-integration";

describe("maintained pixel production integration", () => {
  it("adds the shared native processing panel to the original suite without dropping its interface", () => {
    const source=readFileSync("src-tauri/resources/pixel-cockpit/upstream/src/App.tsx","utf8");
    const result=adoptProcessingPanel(source);
    expect(result).toContain("<PixelProcessingPanel");
    expect(result).toContain("execute={runSuiteOperation}");
    expect(result).toContain("activity={pixelProcessingActivity}");
    expect(result).toContain("<CockpitHealthPanel");
    expect(result).toContain("selectedFrameId={selectedFrameId}");
    expect(result).toContain("cell: processedActionGeometry(action)?.cell ?? STANDARD_ANIMATION_CELL");
    const normalizer=result.slice(result.indexOf("function normalizeAnimationAction(action:"),result.indexOf("function hatchPetSpriteAction"));
    expect(normalizer).toContain("processedActionGeometry(action)");
    expect(result.slice(0,result.indexOf("function normalizeAnimationAction(action:"))).not.toContain("processedActionGeometry(action)");
    expect(result).toContain('selected.source === "generate" || processedAnimation(selected)');
    expect(result).toContain('processedAnimation(selected)?.frames ?? historyAnimationFrameCount(selected)');
    expect(result).toContain('directionId: "poses", action: selectedAnimationAction');
    expect(result).toContain('processedAnimation(selected)?.grid ?? inferAnimationSheetGrid');
    expect(result).toContain('await exportSelectedAnimationPackV2(session.initialPack);');
    expect(()=>adoptProcessingPanel("interface changed")).toThrow("boundary");
  });
  it("enhances the original compiler while retaining all recipe variants and QA code", () => {
    const source=readFileSync("src-tauri/resources/pixel-cockpit/upstream/src/lib/motionRecipes.ts","utf8");
    const result=adoptProductionCompiler(source);
    expect(result).toContain("function compileOriginalMotionRecipe(");
    expect(result).toContain("enhanceMotionRecipe(compileOriginalMotionRecipe(input)");
    expect(result).toContain("+jarvis-production-v1");
    expect(result).toContain("compileMotionRecipeQaContract");
    expect(result).toContain('id: "quadruped"');
    expect(()=>adoptProductionCompiler("interface changed")).toThrow("compiler");
  });
});
