import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, it } from "vitest";
import { readerMessages } from "./monaco-locale";

it("keeps the Portuguese reader controls aligned with the pinned Monaco message indices", () => {
  const root = resolve("node_modules/monaco-editor/esm/vs");
  const source = [
    "base/browser/ui/findinput/findInput.js", "base/browser/ui/findinput/findInputToggles.js",
    "base/browser/ui/findinput/replaceInput.js", "base/browser/ui/inputbox/inputBox.js",
    "editor/contrib/find/browser/findController.js", "editor/contrib/find/browser/findWidget.js",
  ].map(path => readFileSync(resolve(root, path), "utf8")).join("\n");
  const nls = globalThis as typeof globalThis & { _VSCODE_NLS_MESSAGES: Array<string | null> };
  for (const [index, [english, translated]] of Object.entries(readerMessages)) {
    const invocation = source.match(new RegExp(`localize(?:2)?\\(${index}, ([^\\n]+)`))?.[1];
    expect(invocation, `Monaco message ${index} still identifies ${english}`).toBeDefined();
    expect(invocation).toContain(english);
    expect(nls._VSCODE_NLS_MESSAGES[Number(index)]).toBe(translated);
  }
});
