import { readFileSync } from "node:fs";
import path from "node:path";
import { createContext, runInContext } from "node:vm";
import { expect, it, vi } from "vitest";
import { IMPECCABLE_RESOURCE_DIRECTORY, SKILL_DIRECTORY } from "./prepare-impeccable";

const script = readFileSync(path.join(IMPECCABLE_RESOURCE_DIRECTORY, SKILL_DIRECTORY, "scripts/live-browser.js"), "utf8");

function browser(native: boolean) {
  const session = vi.fn(() => undefined);
  const fetch = vi.fn();
  const events = vi.fn();
  const timers = vi.fn();
  const host = {
    ...(native ? { __jarvisBrowser() {} } : {}),
    __IMPECCABLE_TOKEN__: "fixture",
    __IMPECCABLE_PORT__: 18457,
    __IMPECCABLE_LIVE_SESSION__: { createLiveBrowserSessionState: session },
  };
  const context = createContext({ window: host, document, localStorage, fetch, EventSource: events,
    setTimeout: timers, setInterval: timers, console: { error: vi.fn() } });
  return { context, host, session, fetch, events, timers };
}

it("never starts Live UI, transport or session recovery in an external browser, including repeated loads", () => {
  const external = browser(false);
  const html = document.body.innerHTML;
  runInContext(script, external.context);
  runInContext(script, external.context);
  expect(external.host).not.toHaveProperty("__IMPECCABLE_LIVE_INIT__");
  expect(external.session).not.toHaveBeenCalled();
  expect(external.fetch).not.toHaveBeenCalled();
  expect(external.events).not.toHaveBeenCalled();
  expect(external.timers).not.toHaveBeenCalled();
  expect(document.body.innerHTML).toBe(html);
});

it("still enters the Live bootstrap in the native browser", () => {
  const native = browser(true);
  runInContext(script, native.context);
  expect(native.session).toHaveBeenCalledOnce();
});
