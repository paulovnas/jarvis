import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, rename, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { describe, expect, it, vi } from "vitest";

const resources = path.resolve("src-tauri/resources/impeccable");

describe.skipIf(process.env.JARVIS_TEST_NATIVE_ENGINE !== "1")("native Live hot design binding", () => {
  it("does not advertise hot design binding for a helper launched without its host owner", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "jarvis-unbound-design-"));
    await writeFile(path.join(root, "package.json"), '{"name":"unbound-design-fixture"}');
    const env: NodeJS.ProcessEnv = { ...process.env, IMPECCABLE_HOME: path.join(root, "home"), IMPECCABLE_SKILL_DIR: path.join(resources, ".agents/skills/impeccable") };
    for (const key of ["IMPECCABLE_DESIGN_PATH", "IMPECCABLE_DESIGN_ROOT", "IMPECCABLE_PRODUCT_PATH", "IMPECCABLE_PRODUCT_ROOT", "JARVIS_LIVE_OWNER_PATH"]) delete env[key];
    const child = spawn(path.join(resources, process.platform === "win32" ? "bin/impeccable.exe" : "bin/impeccable"), ["live-server"], { cwd: root, env, stdio: "ignore" });
    const closed = new Promise<void>(resolve => child.once("close", () => resolve()));
    try {
      let server: { pid: number; port: number; token: string } | undefined;
      await vi.waitFor(async () => { server = JSON.parse(await readFile(path.join(root, ".impeccable/live/server.json"), "utf8")) as typeof server; expect(server?.pid).toBe(child.pid); }, { timeout: 10_000 });
      if (!server) throw new Error("The native helper did not start");
      const health: unknown = await (await fetch(`http://127.0.0.1:${server.port}/health`)).json();
      expect(health).toMatchObject({ jarvisQueueProtocol: 1, jarvisDesignBindingProtocol: 0 });
    } finally {
      child.kill();
      await closed;
      await rm(root, { recursive: true, force: true });
    }
  }, 20_000);

  it("reads created and relinked designs without replacing the helper or losing a pending alteration", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "jarvis-hot-design-"));
    const app = path.join(root, "app");
    const owner = path.join(root, "home/owner.json");
    await mkdir(app, { recursive: true });
    await mkdir(path.dirname(owner), { recursive: true });
    await writeFile(path.join(app, "package.json"), '{"name":"hot-design-fixture"}');
    await writeFile(path.join(app, "index.html"), '<main id="preview">Original</main>');
    const bind = async (design: string | null) => {
      await writeFile(`${owner}.tmp`, JSON.stringify({ project_root: root, root: app, design_path: design }));
      await rename(`${owner}.tmp`, owner);
    };
    await bind(null);
    const env: NodeJS.ProcessEnv = { ...process.env, IMPECCABLE_HOME: path.join(root, "home"), IMPECCABLE_SKILL_DIR: path.join(resources, ".agents/skills/impeccable"), IMPECCABLE_DESIGN_ROOT: root, JARVIS_LIVE_OWNER_PATH: owner };
    for (const key of ["IMPECCABLE_DESIGN_PATH", "IMPECCABLE_PRODUCT_PATH", "IMPECCABLE_PRODUCT_ROOT"]) delete env[key];
    const child = spawn(path.join(resources, process.platform === "win32" ? "bin/impeccable.exe" : "bin/impeccable"), ["live-server"], { cwd: app, env, stdio: "ignore" });
    const closed = new Promise<void>(resolve => child.once("close", () => resolve()));
    try {
      let server: { pid: number; port: number; token: string } | undefined;
      await vi.waitFor(async () => { server = JSON.parse(await readFile(path.join(app, ".impeccable/live/server.json"), "utf8")) as typeof server; expect(server?.pid).toBe(child.pid); }, { timeout: 10_000 });
      if (!server) throw new Error("The native helper did not start");
      const url = (endpoint: string) => `http://127.0.0.1:${server?.port}/${endpoint}?token=${encodeURIComponent(server?.token ?? "")}`;
      const json = async (endpoint: string): Promise<unknown> => (await fetch(url(endpoint))).json();
      expect((await fetch(url("design-system/raw"))).status).toBe(404);
      const queued = await fetch(url("events"), { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ token: server.token, type: "generate", id: "b0338dda", action: "impeccable", element: { selector: "#preview", outerHTML: '<main id="preview">Original</main>' }, prompt: "Keep working", count: 1 }) });
      expect(queued.status, await queued.text()).toBe(200);
      const before = await json("status");
      expect(before).toMatchObject({ pendingEvents: [{ id: "b0338dda", type: "generate" }] });
      expect(await json("health")).toMatchObject({ jarvisDesignBindingProtocol: 1 });
      const design = path.join(root, "design.md");
      const linked = path.join(root, "linked.md");
      await writeFile(design, '---\nname: Created design\ncolors:\n  primary: "#123456"\n---\n# Created');
      await bind(design);
      const created = await fetch(url("design-system/raw"));
      expect(created.status).toBe(200);
      expect(await created.text()).toContain("Created design");
      await writeFile(linked, '---\nname: Linked design\ncolors:\n  primary: "#abcdef"\n---\n# Linked');
      await bind(linked);
      expect(await json("design-system.json")).toMatchObject({ present: true, hasMd: true, parsed: { frontmatter: { name: "Linked design" } } });
      expect(await json("status")).toEqual(before);
      expect(JSON.parse(await readFile(path.join(app, ".impeccable/live/server.json"), "utf8"))).toEqual(server);
      await mkdir(path.join(app, ".impeccable"), { recursive: true });
      await writeFile(path.join(app, ".impeccable/design.json"), JSON.stringify({ colors: { primary: "#000000" } }));
      await bind(path.join(os.tmpdir(), "outside-design.md"));
      expect((await fetch(url("design-system/raw"))).status).toBe(503);
      const invalid = await fetch(url("design-system.json"));
      expect(invalid.status).toBe(503);
      expect(await invalid.json()).toMatchObject({ present: false });
      expect(await json("status")).toEqual(before);
    } finally {
      child.kill();
      await closed;
      await rm(root, { recursive: true, force: true });
    }
  }, 20_000);
});
