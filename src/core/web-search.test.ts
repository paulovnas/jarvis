import { expect, it } from "vitest";
import { readWebSearchResult, webSearchConfigSchema } from "./web-search";

it("reads Brave Search snippets and sources without inventing an AI model", () => {
  const result = { accountAlias: "brave-search-work", provider: "Brave Search", model: null, answer: "Official documentation", sources: [{ title: "Tauri", url: "https://tauri.app/" }] };
  expect(readWebSearchResult(JSON.stringify(result))).toEqual(result);
  expect(webSearchConfigSchema.parse({ accountAlias: result.accountAlias, model: null, inheritChat: false }).model).toBeNull();
  expect(readWebSearchResult(JSON.stringify({ ...result, sources: [{ title: "Unsafe", url: "javascript:alert(1)" }] }))).toBeNull();
});

it("preserves Muse search independently of the chat account and defaults legacy searches to Jarvis", () => {
  const muse = { executor: "muse", accountAlias: null, model: "muse-wonder", inheritChat: false };
  expect(webSearchConfigSchema.parse(muse)).toEqual(muse);
  expect(webSearchConfigSchema.parse({ accountAlias: "work" }).executor).toBe("jarvis");
  expect(webSearchConfigSchema.safeParse({ ...muse, executor: "claude" }).success).toBe(false);
});
