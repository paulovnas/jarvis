/// <reference types="node" />
import { mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { execFile } from "node:child_process";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { expect, it } from "vitest";

it("packages shared component styles and usable icons for the toolbar, page and extension", async () => {
  const output = await mkdtemp(path.join(tmpdir(), "jarvis-extension-test-"));
  try {
    // Keep esbuild in its native Node realm, outside the DOM test environment.
    await promisify(execFile)(process.execPath, [path.resolve("node_modules/vite/bin/vite.js"), "build", "--config", "vite.extension.config.ts", "--outDir", output, "--logLevel", "silent"]);
    const manifest: { icons: Record<string, string>; action: { default_icon: Record<string, string> } } = JSON.parse(await readFile(path.join(output, "manifest.json"), "utf8"));
    expect(Object.keys(manifest.icons)).toEqual(expect.arrayContaining(["16", "32", "48", "128"]));
    expect(Object.keys(manifest.action.default_icon)).toEqual(expect.arrayContaining(["16", "32"]));
    for (const [size, file] of Object.entries({ ...manifest.icons, ...manifest.action.default_icon })) {
      const png = await readFile(path.join(output, file));
      expect(png.subarray(1, 4).toString()).toBe("PNG");
      expect(png.readUInt32BE(16)).toBeGreaterThanOrEqual(Number(size));
    }
    const html = await readFile(path.join(output, "options.html"), "utf8");
    expect(html).toContain('rel="icon"');
    expect(html).toContain("./icons/32.png");
    const cssFiles = (await readdir(path.join(output, "assets"))).filter(file => file.endsWith(".css"));
    const css = (await Promise.all(cssFiles.map(file => readFile(path.join(output, "assets", file), "utf8")))).join("\n");
    // Without the shared source directory, these registry layout rules vanish.
    expect(css).toContain(".inline-flex");
    expect(css).toContain(".flex-col");
    expect(css).toContain("--card-spacing:");
  } finally {
    await rm(output, { recursive: true, force: true });
  }
}, 30_000);

it("builds a Firefox event page with its own permissions and no Chromium debugger dependency", async () => {
  const output = await mkdtemp(path.join(tmpdir(), "jarvis-firefox-test-"));
  try {
    await promisify(execFile)(process.execPath, [path.resolve("node_modules/vite/bin/vite.js"), "build", "--config", "vite.extension.config.ts", "--mode", "firefox", "--outDir", output, "--logLevel", "silent"]);
    const manifest: { permissions: string[]; host_permissions: string[]; background: { scripts: string[]; type: string; service_worker?: string }; browser_specific_settings: { gecko: { id: string; strict_min_version: string; data_collection_permissions: { required: string[] } } } } = JSON.parse(await readFile(path.join(output, "manifest.json"), "utf8"));
    expect(manifest.permissions).toEqual(expect.arrayContaining(["scripting", "webNavigation", "webRequest", "webRequestBlocking", "webRequestFilterResponse"]));
    expect(manifest.permissions).not.toContain("debugger");
    // Firefox exposes captureTab only with this exact permission, even when
    // HTTP(S) patterns already cover the same web pages. Runtime guards remain.
    expect(manifest.host_permissions).toEqual(["<all_urls>"]);
    const native: { bundle: { resources: Record<string, string> } } = JSON.parse(await readFile(path.resolve("src-tauri/tauri.conf.json"), "utf8"));
    expect(native.bundle.resources["../browser-extension/dist-firefox/"]).toBe("browser-extension-firefox/");
    expect(native.bundle.resources["../browser-extension/dist/"]).toBe("browser-extension/");
    expect(manifest.background).toEqual({ scripts: ["worker.js"], type: "module", persistent: false });
    expect(manifest.browser_specific_settings.gecko).toMatchObject({ id: "browser@jarvis.foxtag.com", strict_min_version: "140.0" });
    expect(manifest.browser_specific_settings.gecko.data_collection_permissions.required).toEqual(["browsingActivity", "websiteContent", "websiteActivity"]);
    const worker = await readFile(path.join(output, manifest.background.scripts[0]), "utf8");
    expect(worker.length).toBeGreaterThan(100);
    expect(worker).not.toContain("chrome.debugger");
    expect(await readFile(path.join(output, "options.html"), "utf8")).toContain("./icons/32.png");
  } finally {
    await rm(output, { recursive: true, force: true });
  }
}, 30_000);
