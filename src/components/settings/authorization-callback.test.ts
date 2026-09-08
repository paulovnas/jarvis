import { runInNewContext } from "node:vm";
import { fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import callbackScript from "./authorization-callback.ts?raw";

function mountCallback(platform = "Win32") {
  document.body.innerHTML = `<button type="button" id="close-tab">Fechar aba</button><p id="close-feedback" role="status" tabindex="-1" hidden></p>`;
  const browser = { close: vi.fn<() => void>(), closed: false, setTimeout: window.setTimeout.bind(window) };
  // Execute the exact script embedded by Rust, without TypeScript transpilation.
  runInNewContext(callbackScript, { window: browser, document, navigator: { platform }, HTMLButtonElement, HTMLElement });
  return browser;
}

describe("authorization callback tab closing", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.useRealTimers();
    document.body.replaceChildren();
  });

  it("requests closure from the button and avoids showing a failure after the tab closes", () => {
    const browser = mountCallback();
    browser.close.mockImplementation(() => { browser.closed = true; });
    fireEvent.click(screen.getByRole("button", { name: "Fechar aba" }));
    expect(browser.close).toHaveBeenCalledOnce();
    vi.runAllTimers();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it.each([["Win32", "Ctrl + W"], ["MacIntel", "⌘ + W"]])("explains the blocked close action on %s", (platform, shortcut) => {
    const browser = mountCallback(platform);
    const button = screen.getByRole("button", { name: "Fechar aba" });
    fireEvent.click(button);
    expect(browser.close).toHaveBeenCalledOnce();
    expect(button).toBeDisabled();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();

    vi.runAllTimers();
    const feedback = screen.getByRole("status");
    expect(feedback).toHaveTextContent("Seu navegador bloqueou o fechamento pelo botão.");
    expect(feedback).toHaveTextContent(`Use ${shortcut} ou o X da aba e volte ao Jarvis.`);
    expect(feedback).toHaveFocus();
    expect(button).toBeEnabled();
    expect(button).not.toHaveAttribute("aria-busy");
  });

  it("keeps the manual closing guidance available if the browser throws", () => {
    const browser = mountCallback();
    browser.close.mockImplementation(() => { throw new Error("Close blocked"); });
    fireEvent.click(screen.getByRole("button", { name: "Fechar aba" }));
    vi.runAllTimers();
    expect(screen.getByRole("status")).toHaveTextContent("Ctrl + W");
    expect(screen.getByRole("button", { name: "Fechar aba" })).toBeEnabled();
  });
});
