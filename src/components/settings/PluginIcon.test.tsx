import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PluginIcon } from "./PluginIcon";

describe("PluginIcon", () => {
  afterEach(() => vi.unstubAllGlobals());
  it("identifies a package with a readable monogram when no icon is available", () => {
    render(<PluginIcon name="Google Drive" />);
    expect(screen.getByRole("img", { name: "Ícone de Google Drive" })).toHaveTextContent("GD");
  });
  it("keeps the package recognizable when its image fails to load", async () => {
    vi.stubGlobal("Image", class {
      onerror: (() => void) | null = null;
      set src(_value: string) { queueMicrotask(() => this.onerror?.()); }
    });
    render(<PluginIcon name="Linear" src="data:image/png;base64,aWNvbg==" />);
    await waitFor(() => expect(screen.getByRole("img", { name: "Ícone de Linear" })).toHaveTextContent("LI"));
  });
  it("displays the supplied embedded package icon after it loads", async () => {
    vi.stubGlobal("Image", class {
      onload: (() => void) | null = null;
      set src(_value: string) { queueMicrotask(() => this.onload?.()); }
    });
    const src = "data:image/png;base64,aWNvbg==";
    render(<PluginIcon name="Linear" src={src} />);
    expect(await screen.findByAltText("")).toHaveAttribute("src", src);
    expect(screen.getByRole("img", { name: "Ícone de Linear" })).not.toHaveTextContent("LI");
  });
});
