import { render } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { RemoteProviderIcon } from "./RemoteProviderIcon";

vi.mock("../../public/provider-claude.svg?url", () => ({ default: "data:image/svg+xml,%3csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'%3e%3cpath d='M0 0h24v24H0z'/%3e%3c/svg%3e" }));

it.each(["openai-codex", "antigravity", "claude-code", "opencode-go"] as const)("bundles the %s mark into the remote asset surface", kind => {
  const { container } = render(<RemoteProviderIcon kind={kind} />);
  const icon = container.firstElementChild;
  expect(icon).toHaveAttribute("aria-hidden", "true");
  expect(icon).toHaveAttribute("style", expect.stringContaining("--remote-provider-icon"));
  expect(container.querySelector<HTMLElement>("span")?.style.getPropertyValue("--remote-provider-icon")).not.toMatch(/url\(["']?\/provider-/);
});

it("keeps an inline SVG with apostrophes valid as a CSS image URL", () => {
  const { container } = render(<RemoteProviderIcon kind="claude-code" />);
  const value = container.querySelector<HTMLElement>("span")?.style.getPropertyValue("--remote-provider-icon") ?? "";
  expect(value).toContain("xmlns='http://www.w3.org/2000/svg'");
  // The background and mask image properties share the CSS image URL grammar.
  const parser = document.createElement("span").style;
  parser.backgroundImage = value;
  expect(parser.backgroundImage).toContain("data:image/svg+xml");
});

it("keeps a custom provider identifiable with the connection icon", () => {
  const { container } = render(<RemoteProviderIcon kind="custom" />);
  expect(container.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
});
