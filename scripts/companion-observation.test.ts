import { readFileSync } from "node:fs";
import path from "node:path";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const source = readFileSync(path.resolve("src-tauri/src/agent/browser/probe/island.rs"), "utf8");
const expression = source.match(/const OBSERVE_DOM: &str = r#"([\s\S]*?)"#;/)?.[1];
if (!expression) throw new Error("Companion observation expression is missing");
const observe = new Function(`return ${expression}`) as () => Promise<Record<string, unknown>>;
let originalClass = "";
let originalStyle = "";
let originalBodyStyle = "";
let originalCompanion: string | undefined;

beforeEach(() => {
  vi.useFakeTimers();
  originalClass = document.documentElement.className;
  originalStyle = document.documentElement.style.cssText;
  originalBodyStyle = document.body.style.cssText;
  originalCompanion = document.documentElement.dataset.companion;
  document.documentElement.classList.add("dark");
  document.documentElement.dataset.companion = "";
  document.documentElement.style.colorScheme = "dark";
  document.body.innerHTML = '<div id="root"><main class="companion" data-expanded="true"><div class="companion-island"><canvas width="80" height="80"></canvas></div></main></div>';
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  document.documentElement.className = originalClass;
  document.documentElement.style.cssText = originalStyle;
  document.body.style.cssText = originalBodyStyle;
  if (originalCompanion === undefined) delete document.documentElement.dataset.companion;
  else document.documentElement.dataset.companion = originalCompanion;
  document.body.replaceChildren();
});

it("observes the real shell scheme and frame timing without exporting chat text or asset URLs", async () => {
  const secret = "private conversation text";
  const url = "https://private.example/image.png?token=private-token";
  document.querySelector(".companion-island")?.append(secret);
  document.body.style.backgroundImage = `url("${url}")`;
  vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  const pending = observe();
  await vi.advanceTimersByTimeAsync(260);
  const result = await pending;
  expect(result).toMatchObject({ mainFrame: true, companionAttribute: true, dark: true, hidden: false, expanded: true, canvasCount: 1, html: { colorScheme: "dark" }, body: { hasBackgroundImage: true } });
  expect(result.frames).toBeGreaterThan(1);
  expect(result.maxGapMs).toBeGreaterThanOrEqual(0);
  expect(JSON.stringify(result)).not.toContain(secret);
  expect(JSON.stringify(result)).not.toContain(url);
  expect(JSON.stringify(result)).not.toContain("private-token");
});

it("reports a hidden compact shell without modifying its document", async () => {
  document.querySelector(".companion")?.setAttribute("data-expanded", "false");
  vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  const before = document.documentElement.outerHTML;
  const pending = observe();
  await vi.advanceTimersByTimeAsync(260);
  expect(await pending).toMatchObject({ hidden: true, expanded: false });
  expect(document.documentElement.outerHTML).toBe(before);
});

it("distinguishes a fixed canvas from its animated mask and pinned content", async () => {
  const shell = document.querySelector<HTMLDivElement>(".companion-island")!;
  Object.assign(shell.style, { width: "640px", height: "600px", clipPath: "inset(0px 176px 568px 176px round 18px)" });
  shell.innerHTML = '<section class="companion-panel" style="width:640px;height:400px"></section>';
  document.querySelector(".companion")?.setAttribute("data-closing", "true");
  const before = document.documentElement.outerHTML;
  const pending = observe();
  await vi.advanceTimersByTimeAsync(260);
  const result = await pending;
  expect(result).toMatchObject({ closing: true, island: { layoutWidth: "640px", layoutHeight: "600px", clipPath: "inset(0px 176px 568px 176px round 18px)" }, panel: { layoutWidth: "640px", layoutHeight: "400px" } });
  expect(document.documentElement.outerHTML).toBe(before);
});
