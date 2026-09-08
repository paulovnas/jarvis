import * as monaco from "monaco-editor/editor/editor.api";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import "monaco-editor/basic-languages/monaco.contribution";
import "monaco-editor/editor/contrib/find/browser/findController";
import "monaco-editor/editor/contrib/folding/browser/folding";
import "monaco-editor/editor/contrib/bracketMatching/browser/bracketMatching";
import { createTokenizationSupport } from "monaco-editor/languages/features/json/tokenization";

// Only the editor worker is needed for this read-only viewer; language services
// and remote loaders are deliberately not started. Vite packages it locally.
globalThis.MonacoEnvironment = { getWorker: () => new EditorWorker() };
// Reuse the JSON/JSONC tokenizer without importing its validation, formatting,
// schema fetching or language-service worker into a read-only application.
monaco.languages.register({ id: "json", extensions: [".json", ".jsonc"], aliases: ["JSON"] });
monaco.languages.setTokensProvider("json", createTokenizationSupport(true));

export { monaco };
