import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { TitleBar } from "./TitleBar";

const native = vi.hoisted(() => ({ isMaximized: vi.fn(), isFullscreen: vi.fn(), setFullscreen: vi.fn(), toggleMaximize: vi.fn(), onResized: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => native }));
beforeEach(() => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  let full = false;
  native.isMaximized.mockResolvedValue(false);
  native.isFullscreen.mockImplementation(async () => full);
  native.setFullscreen.mockImplementation(async next => { full = next; });
  native.toggleMaximize.mockResolvedValue(undefined);
  native.onResized.mockResolvedValue(() => {});
});
afterEach(() => { vi.restoreAllMocks(); vi.clearAllMocks(); });
it("toggles macOS fullscreen with the green button and preserves titlebar double-click maximize", async () => {
  const user = userEvent.setup(); render(<TitleBar />);
  await waitFor(() => expect(native.onResized).toHaveBeenCalled());
  await user.click(screen.getByRole("button", { name: "Entrar em tela cheia" }));
  expect(native.setFullscreen).toHaveBeenLastCalledWith(true);
  await user.click(screen.getByRole("button", { name: "Sair da tela cheia" }));
  expect(native.setFullscreen).toHaveBeenLastCalledWith(false);
  expect(native.toggleMaximize).not.toHaveBeenCalled();
  await user.dblClick(screen.getByRole("banner"));
  expect(native.toggleMaximize).toHaveBeenCalledOnce();
  expect(native.setFullscreen).toHaveBeenCalledTimes(2);
});
