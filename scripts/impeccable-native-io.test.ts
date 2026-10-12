import { execFileSync } from "node:child_process";
import { chmod, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";

const resources = path.resolve("src-tauri/resources/impeccable");
const source = `<main>
<!-- impeccable-variants-start ab12cd34 -->
<div data-impeccable-variants="ab12cd34" data-impeccable-variant-count="2" style="display: contents">
<!-- Original -->
<div data-impeccable-variant="original"><h1>Original</h1></div>
<!-- Variants: insert below this line -->
<div data-impeccable-variant="1"><h1>First choice</h1></div>
<div data-impeccable-variant="2"><h1>Selected choice</h1></div>
</div>
<!-- impeccable-variants-end ab12cd34 -->
</main>`;

// Opt-in uses the actual bundled engine; file permission semantics differ on Windows.
describe.skipIf(process.env.JARVIS_TEST_NATIVE_ENGINE !== "1" || process.platform === "win32")("native Live source persistence", () => {
  it.each(["accept", "discard"])("does not confirm %s on write failure and permits retry", async operation => {
    const root = await mkdtemp(path.join(os.tmpdir(), "jarvis-live-source-"));
    const file = path.join(root, "index.html");
    const env: NodeJS.ProcessEnv = { ...process.env, IMPECCABLE_HOME: path.join(root, "home"), IMPECCABLE_SKILL_DIR: path.join(resources, ".agents/skills/impeccable") };
    delete env.IMPECCABLE_DESIGN_PATH;
    delete env.IMPECCABLE_DESIGN_ROOT;
    delete env.IMPECCABLE_PRODUCT_PATH;
    delete env.IMPECCABLE_PRODUCT_ROOT;
    const run = () => {
      const output = execFileSync(path.join(resources, "bin/impeccable"), ["live-accept", "--id", "ab12cd34", ...(operation === "discard" ? ["--discard"] : ["--variant", "2", "--no-bake"])], {
        cwd: root, encoding: "utf8", timeout: 15_000,
        env,
      });
      const value: unknown = JSON.parse(output);
      if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Invalid engine result");
      return value as Record<string, unknown>;
    };
    try {
      await writeFile(path.join(root, "package.json"), '{"name":"live-source-fixture"}');
      await writeFile(file, source, { mode: 0o444 });
      const failed = run();
      expect(failed.handled).toBe(false);
      expect(await readFile(file, "utf8")).toBe(source);
      expect(await stat(path.join(root, ".impeccable/live/accept-receipts/ab12cd34.json")).catch(() => null)).toBeNull();
      await chmod(file, 0o644);
      const retried = run();
      expect(retried.handled).toBe(true);
      expect(retried.alreadyApplied).not.toBe(true);
      const saved = await readFile(file, "utf8");
      expect(saved).toContain(operation === "discard" ? "Original" : "Selected choice");
      expect(saved).not.toContain("First choice");
      expect(saved).not.toContain("impeccable-variants-start");
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});
