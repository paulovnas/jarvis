import { expect, it } from "vitest";
import { webSearchConfigSchema } from "./web-search";

it("preserves Muse search independently of the chat account and defaults legacy searches to Jarvis", () => {
  const muse = { executor: "muse", accountAlias: null, model: "muse-wonder", inheritChat: false };
  expect(webSearchConfigSchema.parse(muse)).toEqual(muse);
  expect(webSearchConfigSchema.parse({ accountAlias: "work" }).executor).toBe("jarvis");
  expect(webSearchConfigSchema.safeParse({ ...muse, executor: "claude" }).success).toBe(false);
});
