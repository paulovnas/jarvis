import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";
import ts from "typescript";
import { promptExampleLabels } from "./prompt-example-labels";

it("translates every bundled example title, summary and category without rewriting prompts", () => {
  const root = "src-tauri/resources/pixel-cockpit/upstream";
  const source = readFileSync(join(root, "src/App.tsx"), "utf8");
  const file = ts.createSourceFile("App.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const labels = new Set<string>();
  function localized(node: ts.Node) {
    if (ts.isPropertyAssignment(node) && node.name.getText(file) === "en" && ts.isStringLiteral(node.initializer)) {
      labels.add(node.initializer.text);
    } else ts.forEachChild(node, localized);
  }
  function visit(node: ts.Node) {
    if (ts.isPropertyAssignment(node) && ["title", "summary", "category"].includes(node.name.getText(file))) {
      localized(node.initializer);
    } else if (ts.isVariableDeclaration(node) && /_CATEGORY$/.test(node.name.getText(file)) && node.initializer) {
      localized(node.initializer);
    } else ts.forEachChild(node, visit);
  }
  visit(file);
  const directory = join(root, "docs/prompt-examples");
  for (const name of readdirSync(directory).filter(name => name.endsWith(".md"))) {
    const markdown = readFileSync(join(directory, name), "utf8").replaceAll("\r\n", "\n");
    for (const match of markdown.matchAll(/###\s+\d+\.\s+([^\n]+)\n\n```text\n/g)) {
      labels.add(match[1].split(/\s+\/\s+/)[0].trim());
    }
  }
  expect(labels.size).toBeGreaterThan(145);
  expect([...labels].filter(label => !promptExampleLabels[label])).toEqual([]);
  expect(promptExampleLabels["Two-Head Chibi Knight"]).toContain("duas cabeças");
  expect(promptExampleLabels["Clockwork Mushroom Courier"]).toBe("Cogumelo mensageiro mecânico");
});
