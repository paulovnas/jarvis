import { expect, it } from "vitest";
import { defaultReasoning, reasoningLabel, selectableReasoningLevels } from "./reasoning";

it("excludes the Ultra alias without changing provider levels or legacy labels", () => {
  const levels = ["none", "low", "ultra", "xhigh", "max", "future"];
  expect(selectableReasoningLevels(levels)).toEqual(["none", "low", "xhigh", "max", "future"]);
  expect(levels).toContain("ultra");
  expect(reasoningLabel("ultra")).toBe("Ultra");
});

it.each([
  { levels: ["ultra", "high", "max"], preferred: "ultra", expected: "max" },
  { levels: ["low", "xhigh", "ultra"], preferred: "ultra", expected: "xhigh" },
  { levels: ["ultra"], preferred: "ultra", expected: null },
  { levels: ["ultra", "low", "high"], preferred: null, expected: "low" },
  { levels: ["low", "high", "ultra"], preferred: "high", expected: "high" },
  { levels: [], preferred: null, expected: null },
])("defaults to $expected for $levels with preference $preferred", ({ levels, preferred, expected }) => {
  expect(defaultReasoning({ reasoningLevels: levels, defaultReasoningLevel: preferred })).toBe(expected);
});
