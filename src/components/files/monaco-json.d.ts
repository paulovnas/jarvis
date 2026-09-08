declare module "monaco-editor/languages/features/json/tokenization" {
  import type { languages } from "monaco-editor/editor/editor.api";
  // Monaco exports the tokenizer module but only publishes declarations for its
  // full language service. Keep this narrow adapter typed to the public contract.
  export function createTokenizationSupport(supportComments: boolean): languages.TokensProvider;
}
