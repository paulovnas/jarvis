import { expect, it } from "vitest";
import { questionRequestSchema } from "./questions";

it("rejects ambiguous recommendations while keeping an explicit default optional", () => {
  expect(questionRequestSchema.safeParse({ questions: [{ id: "one", question: "Escolha?", options: [
    { label: "A", recommended: true }, { label: "B", recommended: true },
  ] }] }).success).toBe(false);
  expect(questionRequestSchema.safeParse({ questions: [{ id: "one", question: "Escolha?", options: [
    { label: "A" }, { label: "B" },
  ] }] }).success).toBe(true);
});
