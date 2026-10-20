import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import ts from "typescript";
import { localizePortugueseCopy, portugueseUiCopy, portugueseWorkflowFormCopy } from "./portuguese-copy";

describe("Pixel Art Portuguese copy", () => {
  it("covers every original shared UI label, including failures and frame editing", () => {
    const source = readFileSync("src-tauri/resources/pixel-cockpit/upstream/src/App.tsx", "utf8");
    const file = ts.createSourceFile("App.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
    const keys: string[] = [];
    function visit(node: ts.Node) {
      if (ts.isVariableDeclaration(node) && node.name.getText(file) === "baseUiCopy" && node.initializer) {
        function collect(child: ts.Node) {
          if (ts.isPropertyAssignment(child) && child.name.getText(file) === "en" && ts.isObjectLiteralExpression(child.initializer)) {
            keys.push(...child.initializer.properties.filter(ts.isPropertyAssignment).map((property) => property.name.getText(file)));
          } else ts.forEachChild(child, collect);
        }
        collect(node.initializer);
      } else ts.forEachChild(node, visit);
    }
    visit(file);
    expect(keys.length).toBeGreaterThan(200);
    expect(keys.filter((key) => !portugueseUiCopy[key])).toEqual([]);
    expect(portugueseUiCopy.codexFailureImportFailedSuggestion).toContain("preservados");
    expect(portugueseUiCopy.moveFrameLeft).toBe("Mover quadro para a esquerda");
  });

  it("localizes all five form modes while preserving original prompts and log text", () => {
    expect(Object.keys(portugueseWorkflowFormCopy)).toHaveLength(5);
    const source = 'const prompt = "Generate pixel art"; const log = "runner stdout"; function withUiCopy(){} function withWorkflowFormCopy(){}';
    const translated = localizePortugueseCopy(source);
    expect(translated).toContain('const prompt = "Generate pixel art"');
    expect(translated).toContain('const log = "runner stdout"');
    expect(translated).toContain('"effect-animation":{"promptLabel":"Instruções do efeito"');
    expect(translated).toContain('Object.assign(uiCopy["pt-BR"]');
  });
});
