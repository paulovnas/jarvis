import { readFileSync } from "node:fs";
import { expect, it } from "vitest";
import ts from "typescript";
import { assetLabels } from "./asset-labels";
import { localizedLabels } from "./localized-labels";
import { advancedLabels } from "./advanced-labels";

it("covers every original motion recipe, topology, effect and profile display label", () => {
  const root = "src-tauri/resources/pixel-cockpit/upstream/src/";
  const labels = {...assetLabels, ...localizedLabels, ...advancedLabels};
  const texts: string[] = [];
  const collect = (source: string, properties: string[]) => {
    const file = ts.createSourceFile("labels.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
    const visit = (node: ts.Node) => {
      if (ts.isPropertyAssignment(node) && properties.includes(node.name.getText(file))
        && ts.isStringLiteral(node.initializer)) texts.push(node.initializer.text);
      ts.forEachChild(node, visit);
    };
    visit(file);
  };
  collect(readFileSync(root+"lib/motionRecipes.ts","utf8"), ["en"]);
  const app = readFileSync(root+"App.tsx","utf8");
  collect(app.slice(app.indexOf("const effectCategoryDefinitions"), app.indexOf("interface PromptExample")), ["en","label"]);
  const profiles = readFileSync(root+"lib/animationTournament.ts","utf8");
  collect(profiles.slice(profiles.indexOf("export const ANIMATION_GENERATION_PROFILES"),profiles.indexOf("export function animationGenerationProfileDefinition")), ["label","description"]);
  expect(texts.length).toBeGreaterThan(100);
  expect([...new Set(texts)].filter((text) => !labels[text])).toEqual([]);
});
