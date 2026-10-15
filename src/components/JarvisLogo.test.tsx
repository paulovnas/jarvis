import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { JarvisLogo } from "./JarvisLogo";

it("uses the new transparent vector mark by default", () => {
  render(<JarvisLogo alt="Jarvis" />);
  const logo = screen.getByRole("img", { name: "Jarvis" });
  expect(logo).toHaveAttribute("src", "/v2/svg/inset-mark.svg");
  expect(logo).toHaveAttribute("draggable", "false");
  expect(existsSync(path.join(process.cwd(), "public", logo.getAttribute("src")!))).toBe(true);
});

it("ships the requested README banner and removes the obsolete public logos", () => {
  expect(readFileSync("README.md", "utf8")).toContain('src="public/v2/png/jarvis-banner@2x.png"');
  expect(existsSync("public/v2/png/jarvis-banner@2x.png")).toBe(true);
  for (const name of ["icon", "horizontal", "vertical"]) {
    expect(existsSync(`public/logo_${name}.png`)).toBe(false);
    expect(existsSync(`public/logo_${name}_background.png`)).toBe(false);
  }
});

it.each([["32x32.png", "png/icon-32.png"], ["64x64.png", "png/icon-64.png"], ["128x128.png", "png/icon-128.png"], ["128x128@2x.png", "png/icon-256.png"], ["icon.png", "png/icon-512.png"], ["icon.ico", "jarvis.ico"]])("keeps native and extension asset %s on the supplied identity", (native, supplied) => {
  expect(readFileSync(`src-tauri/icons/${native}`).equals(readFileSync(`public/v2/${supplied}`))).toBe(true);
});
