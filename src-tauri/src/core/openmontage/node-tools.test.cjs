const { test } = require("node:test");
const assert = require("node:assert/strict");
const { mkdtempSync, mkdirSync, copyFileSync, writeFileSync, rmSync, realpathSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join, dirname } = require("node:path");
const { spawnSync } = require("node:child_process");

test("managed renderers resolve outside their package and preserve paths with spaces", () => {
  const generation = mkdtempSync(join(tmpdir(), "jarvis montage "));
  try {
    const dispatcher = join(generation, "bin/jarvis-npx.cjs");
    mkdirSync(dirname(dispatcher), { recursive: true });
    copyFileSync(join(__dirname, "node-tools.cjs"), dispatcher);
    const argsSource = "process.stdout.write(JSON.stringify({args:process.argv.slice(2),cwd:process.cwd()}));";
    for (const entry of [
      "node_modules/hyperframes/bin/hyperframes.mjs",
      "OpenMontage/remotion-composer/node_modules/@remotion/cli/remotion-cli.js",
    ]) {
      const path = join(generation, entry);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, argsSource);
    }
    const composition = join(generation, "user project");
    mkdirSync(composition);
    const env = { ...process.env, REMOTION_BROWSER_EXECUTABLE: join(generation, "browser with space") };
    writeFileSync(env.REMOTION_BROWSER_EXECUTABLE, "fixture browser");
    const hf = spawnSync(process.execPath, [dispatcher, "--yes", "hyperframes", "render", "--output", "video with space.mp4"], { cwd: composition, env, encoding: "utf8" });
    assert.equal(hf.status, 0, hf.stderr);
    assert.deepEqual(JSON.parse(hf.stdout), { args: ["render", "--output", "video with space.mp4"], cwd: realpathSync(composition) });
    const remotion = spawnSync(process.execPath, [dispatcher, "remotion", "render", "index.tsx", "Explainer", "out.mp4"], { cwd: composition, env, encoding: "utf8" });
    assert.equal(remotion.status, 0, remotion.stderr);
    assert.equal(JSON.parse(remotion.stdout).args.at(-1), `--browser-executable=${env.REMOTION_BROWSER_EXECUTABLE}`);
    assert.ok(JSON.parse(remotion.stdout).args.includes("--bundle-cache=false"));
    const ensure = spawnSync(process.execPath, [dispatcher, "remotion", "browser", "ensure"], { cwd: composition, env, encoding: "utf8" });
    assert.equal(ensure.status, 0, ensure.stderr);
    assert.match(ensure.stdout, /managed browser is ready/);
    const absent = spawnSync(process.execPath, [dispatcher, "remotion", "ensure-browser"], { cwd: composition, env: { ...env, REMOTION_BROWSER_EXECUTABLE: "" }, encoding: "utf8" });
    assert.equal(absent.status, 1);
    assert.match(absent.stderr, /Repair OpenMontage/);
    const unknown = spawnSync(process.execPath, [dispatcher, "uninstalled-package"], { cwd: composition, env, encoding: "utf8" });
    assert.equal(unknown.status, 1);
    assert.match(unknown.stderr, /Configure additional dependencies explicitly/);
  } finally {
    rmSync(generation, { recursive: true, force: true });
  }
});
