/** Keep the upstream snapshot unchanged; fail explicitly if a future adoption moves these boundaries. */
export function adoptProcessingPanel(source: string) {
  source=source.replace(/\r\n/g,"\n");
  const boundary = "          <CockpitHealthPanel";
  if (!source.includes(boundary)) throw new Error("Pixel processing UI boundary changed; review the adoption.");
  const geometry = /(function normalizeAnimationAction\(action: SpriteAction\): SpriteAction \{[\s\S]*?)cell: STANDARD_ANIMATION_CELL,\s*anchor: STANDARD_ANIMATION_ANCHOR/;
  if (!geometry.test(source)) throw new Error("Animation action normalization changed; review the adoption.");
  const integrations=[
    ['selected && selected.source === "generate" && selectedAnimationFrames.length > 0','selected && (selected.source === "generate" || processedAnimation(selected)) && selectedAnimationFrames.length > 0'],
    ['const selectedAnimationFramesPerDirection = historyAnimationFrameCount(selected);','const selectedAnimationFramesPerDirection = processedAnimation(selected)?.frames ?? historyAnimationFrameCount(selected);'],
    ['() => inferAnimationSheetGrid(selectedAnimationFrames, selectedAnimationVariant, selectedAnimationFramesPerDirection),','() => processedAnimation(selected)?.grid ?? inferAnimationSheetGrid(selectedAnimationFrames, selectedAnimationVariant, selectedAnimationFramesPerDirection),'],
    ['[selectedAnimationFrames, selectedAnimationVariant, selectedAnimationFramesPerDirection]','[selected, selectedAnimationFrames, selectedAnimationVariant, selectedAnimationFramesPerDirection]'],
    ['() =>\n      selectedAnimationVariant === "directional-hatch-pet"','() =>\n      processedAnimation(selected) ? [{ directionId: "poses", action: selectedAnimationAction }] : selectedAnimationVariant === "directional-hatch-pet"'],
    ['if (!item) return false;\n  if (isEffectAnimationHistoryItem(item)) return false;','if (!item) return false;\n  if (processedAnimation(item)) return true;\n  if (isEffectAnimationHistoryItem(item)) return false;'],
    ['anchor: firstFrame ? animationExportAnchor(cell.width, cell.height) : activeAction.anchor','anchor: processedActionGeometry(activeAction)?.anchor ?? (firstFrame ? animationExportAnchor(cell.width, cell.height) : activeAction.anchor)'],
    ['const recipeDefault = createAnimationPackV2({','const recipeDefault = processedAnimationPack(selected, createAnimationPackV2({'],
  ];
  for(const [before,after] of integrations) {
    if(!source.includes(before))throw new Error("Processed animation export boundary changed; review the adoption.");
    source=source.replace(before,after);
  }
  source=source.replace(/(const recipeDefault = processedAnimationPack\(selected, createAnimationPackV2\(\{[\s\S]*?)\}\);\n {4}const initialPack/,'$1}));\n    const initialPack');
  const exportBoundary=/(async function exportSelectedAnimationPack\(\) \{[\s\S]*?try \{\n)/;
  if(!exportBoundary.test(source))throw new Error("Processed animation pack export changed; review the adoption.");
  source=source.replace(exportBoundary,'$1      if (processedAnimation(selected)) {\n        const session = buildSelectedAnimationPackV2();\n        if (!session) throw new Error("Não foi possível preparar as poses para exportação.");\n        await exportSelectedAnimationPackV2(session.initialPack);\n        return;\n      }\n');
  return 'import { PixelProcessingPanel } from "@/components/pixel-art/PixelProcessingPanel";\nimport { processedActionGeometry, processedAnimation, processedAnimationPack } from "../processing-tools";\nimport { runSuiteOperation, pixelProcessingActivity } from "../jarvis-worker";\n' +
    source.replace(geometry, "$1cell: processedActionGeometry(action)?.cell ?? STANDARD_ANIMATION_CELL,\n    anchor: processedActionGeometry(action)?.anchor ?? STANDARD_ANIMATION_ANCHOR")
      .replace(boundary, '          <PixelProcessingPanel selected={selected} selectedFrameId={selectedFrameId} history={history} frames={frames} grid={grid} busy={isBusy || codexJobs.length > 0} execute={runSuiteOperation} activity={pixelProcessingActivity}/>\n' + boundary);
}

export function adoptProductionCompiler(source: string) {
  if (!source.includes("export function compileMotionRecipe(input:")) throw new Error("Motion recipe compiler changed; review the adoption.");
  const version = /(export const MOTION_RECIPE_COMPILER_VERSION\s*=\s*)(["'])([^"']+)\2/;
  if (!version.test(source)) throw new Error("Motion recipe compiler version changed; review the adoption.");
  return 'import { enhanceMotionRecipe } from "../../production-recipes";\n' +
    source.replace(version, (_match, prefix: string, quote: string, value: string) => prefix + quote + value + "+jarvis-production-v1" + quote)
      .replace("export function compileMotionRecipe(input:", "function compileOriginalMotionRecipe(input:") +
    '\nexport function compileMotionRecipe(input: Parameters<typeof compileOriginalMotionRecipe>[0]): ReturnType<typeof compileOriginalMotionRecipe> {\n' +
    '  return enhanceMotionRecipe(compileOriginalMotionRecipe(input), input.recipe.id, input.topology ?? input.recipe.defaultTopology);\n}\n';
}
