import { render } from "@testing-library/react";
import { expect, it } from "vitest";
import { ProviderIcon } from "./ProviderIcon";
import openCodeMark from "../../public/provider-opencode-go.svg?raw";

it("loads OpenCode branding with the same compact size and inherited color as other providers", () => {
  const { container } = render(<ProviderIcon kind="opencode-go" className="size-4 text-primary" />);
  expect(container.firstElementChild).toHaveAttribute("aria-hidden", "true");
  expect(container.firstElementChild).toHaveClass("bg-current", "size-4", "text-primary");
  expect(container.firstElementChild).toHaveAttribute("style", expect.stringContaining("provider-opencode-go.svg"));
});

it("keeps the OpenCode mark scalable and mask-compatible without embedded images or fixed colors", () => {
  const svg = new DOMParser().parseFromString(openCodeMark, "image/svg+xml");
  expect(svg.querySelector("parsererror")).toBeNull();
  expect(svg.documentElement.getAttribute("viewBox")).toBe("0 0 24 24");
  expect(svg.documentElement.getAttribute("fill")).toBe("none");
  expect(svg.querySelector("image, script, foreignObject")).toBeNull();
  const painted = [...svg.querySelectorAll("[fill]")].filter(element => element.getAttribute("fill") !== "none");
  expect(painted.length).toBeGreaterThan(0);
  expect(painted.every(element => element.getAttribute("fill") === "currentColor")).toBe(true);
});
