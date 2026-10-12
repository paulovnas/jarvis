import { execFileSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

const resources = path.resolve("src-tauri/resources/impeccable");
const roots: string[] = [];
afterEach(async () => { await Promise.all(roots.splice(0).map(root => rm(root, { recursive: true, force: true }))); });

async function fixture(): Promise<{ root: string; env: NodeJS.ProcessEnv; run: () => string }> {
  const root = await mkdtemp(path.join(os.tmpdir(), "jarvis-native-product-"));
  roots.push(root);
  await mkdir(path.join(root, ".git"));
  await mkdir(path.join(root, "app"));
  await mkdir(path.join(root, ".jarvis/knowledge"), { recursive: true });
  await writeFile(path.join(root, "PRODUCT.md"), "# Competing root product");
  await writeFile(path.join(root, "app/PRODUCT.md"), "# Competing child product");
  await writeFile(path.join(root, "app/package.json"), '{"name":"native-product-fixture"}');
  await writeFile(path.join(root, ".jarvis/knowledge/prd.md"), "# Canonical Knowledge PRD\n\n## Platform\n\nandroid\n");
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    IMPECCABLE_PRODUCT_ROOT: root,
    IMPECCABLE_PRODUCT_PATH: path.join(root, ".jarvis/knowledge/prd.md"),
    IMPECCABLE_HOME: path.join(root, "home"),
    IMPECCABLE_NO_UPDATE_CHECK: "1",
    IMPECCABLE_SKILL_DIR: path.join(resources, ".agents/skills/impeccable"),
  };
  delete env.IMPECCABLE_DESIGN_PATH;
  delete env.IMPECCABLE_DESIGN_ROOT;
  delete env.IMPECCABLE_CONTEXT_DIR;
  const run = () => execFileSync(path.join(resources, "bin", process.platform === "win32" ? "impeccable.exe" : "impeccable"), ["context"], {
    cwd: path.join(root, "app"), env, encoding: "utf8", timeout: 15_000,
  });
  return { root, env, run };
}

describe.skipIf(process.env.JARVIS_TEST_NATIVE_ENGINE !== "1")("native Impeccable canonical product", () => {
  it("reads Knowledge PRD from a child app without modifying competing PRODUCT files", async () => {
    const { root, run } = await fixture();
    const output = run();
    expect(output).toContain("Canonical Knowledge PRD");
    expect(output).toContain(".jarvis/knowledge/prd.md");
    expect(output).toContain('"platform": "android"');
    expect(output).not.toContain("Competing child product");
    expect(output).not.toContain("Competing root product");
    expect(await readFile(path.join(root, "PRODUCT.md"), "utf8")).toBe("# Competing root product");
    expect(await readFile(path.join(root, "app/PRODUCT.md"), "utf8")).toBe("# Competing child product");
  });

  it("rejects a missing bound PRD instead of silently reading another PRODUCT", async () => {
    const { root, run } = await fixture();
    await rm(path.join(root, ".jarvis/knowledge/prd.md"));
    expect(run).toThrow("documento de produto configurado não existe");
  });

  it("retains upstream product discovery when the host supplies no binding", async () => {
    const { env, run } = await fixture();
    delete env.IMPECCABLE_PRODUCT_PATH;
    delete env.IMPECCABLE_PRODUCT_ROOT;
    expect(run()).toContain("Competing child product");
  });
});
