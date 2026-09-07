import { expect, it, vi } from "vitest";
import { command } from "./release-common";

// The shared DOM test environment rewrites import.meta asset URLs. These tests
// exercise subprocess behavior, using the project cwd instead of URL discovery.
vi.mock("node:url", async importOriginal => {
  const actual = await importOriginal<typeof import("node:url")>();
  const url = { ...actual, fileURLToPath: () => process.cwd() };
  return { ...url, default: url };
});

it("runs children without pagers or interactive prompts and preserves the caller environment", () => {
  const env = {
    ...process.env, GH_PAGER: "less", GIT_PAGER: "less", PAGER: "less", GH_FORCE_TTY: "80",
    GH_PROMPT_DISABLED: "0", GIT_TERMINAL_PROMPT: "1", JARVIS_TEST_CONTEXT: "retained",
  };
  const original = { ...env };
  const output = command(process.execPath, ["-e", `
    const names = ["GH_PAGER", "GIT_PAGER", "PAGER", "GH_FORCE_TTY", "GH_PROMPT_DISABLED", "GIT_TERMINAL_PROMPT", "GH_NO_UPDATE_NOTIFIER", "JARVIS_TEST_CONTEXT"];
    process.stdout.write(JSON.stringify(names.map(name => process.env[name] ?? null)));
  `], true, env);
  expect(JSON.parse(output)).toEqual(["cat", "cat", "cat", null, "1", "0", "1", "retained"]);
  expect(env).toEqual(original);
});

it("closes child stdin even when command output streams to the terminal", () => {
  expect(() => command(process.execPath, ["-e", `
    if (require("node:fs").readFileSync(0, "utf8") !== "") process.exit(1);
  `])).not.toThrow();
});
