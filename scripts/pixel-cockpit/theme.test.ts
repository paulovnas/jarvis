import { expect, it } from "vitest";
import { adoptTheme } from "./theme";

it("adapts hardcoded light UI surfaces without changing image previews or palette swatches", () => {
  const output = adoptTheme(".panel { background: #ffffff; color: #17211b; }\n.swatch { background: #ffffff; }\n.preview-background.light { background: #fff; color: #17211b; }");
  expect(output).toContain(".panel { background: #2b2d2e; color: #ededed;");
  expect(output).toContain(".swatch { background: #ffffff; }");
  expect(output).toContain(".preview-background.light { background: #fff; color: #17211b; }");
});
