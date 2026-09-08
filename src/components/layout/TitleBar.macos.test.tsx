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
it("shows macOS traffic-light controls before the wordmark", () => {
  render(<TitleBar />);
  const buttons = screen.getAllByRole("button");
  expect(buttons.map(button => button.getAttribute("aria-label"))).toEqual(["Fechar janela", "Minimizar janela", "Entrar em tela cheia"]);
  const logo = screen.getByRole("img", { name: "Jarvis" });
  expect(buttons[2].compareDocumentPosition(logo) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});
it("toggles macOS fullscreen with the green button and hides the custom titlebar", async () => {
  const user = userEvent.setup(); render(<TitleBar />);
  await waitFor(() => expect(native.onResized).toHaveBeenCalled());
  await user.dblClick(screen.getByRole("banner"));
  expect(native.toggleMaximize).toHaveBeenCalledOnce();
  await user.click(screen.getByRole("button", { name: "Entrar em tela cheia" }));
  expect(native.setFullscreen).toHaveBeenLastCalledWith(true);
  await waitFor(() => expect(screen.queryByRole("banner")).not.toBeInTheDocument());
});

it("keeps the custom titlebar hidden when macOS already starts fullscreen", async () => {
  native.isFullscreen.mockResolvedValue(true);
  render(<TitleBar />);
  await waitFor(() => expect(screen.queryByRole("banner")).not.toBeInTheDocument());
});
