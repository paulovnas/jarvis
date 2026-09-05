import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { DesktopLayoutProvider } from "./DesktopLayoutProvider";
import { useDesktopLayout } from "@/hooks/use-desktop-layout";
import { DEFAULT_DESKTOP_LAYOUT, type DesktopLayout } from "@/core/desktop-layout";
import { Button } from "@/components/ui/button";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));

function Controls() {
  const { layout, updateLayout } = useDesktopLayout();
  return <><p>{layout.inspectorTab} / {layout.settingsTab} / {layout.panels["home-sidebar-panel"]}</p>
    <Button onClick={() => updateLayout({ inspectorTab: "details" })}>Detalhes</Button>
    <Button onClick={() => updateLayout({ settingsTab: "skills" })}>Skills</Button>
  </>;
}
beforeEach(() => vi.clearAllMocks());

it("restores layout before showing controls and keeps sequential changes across remounts", async () => {
  let saved: DesktopLayout = { ...DEFAULT_DESKTOP_LAYOUT, panels: { "home-sidebar-panel": 23, "home-main-panel": 51, "home-inspector-panel": 26 } };
  vi.mocked(invoke).mockImplementation((command, args) => {
    if (command === "save_desktop_layout") saved = (args as { layout: DesktopLayout }).layout;
    return Promise.resolve(command === "get_desktop_layout" ? saved : undefined);
  });
  const user = userEvent.setup();
  const first = render(<DesktopLayoutProvider><Controls /></DesktopLayoutProvider>);
  expect(screen.getByRole("status", { name: "Carregando Jarvis" })).toBeVisible();
  await screen.findByText("activities / general / 23");
  await user.click(screen.getByRole("button", { name: "Detalhes" }));
  await user.click(screen.getByRole("button", { name: "Skills" }));
  await waitFor(() => expect(saved.settingsTab).toBe("skills"));
  expect(saved.inspectorTab).toBe("details");
  first.unmount();
  render(<DesktopLayoutProvider><Controls /></DesktopLayoutProvider>);
  expect(await screen.findByText("details / skills / 23")).toBeVisible();
});

it("does not overwrite unavailable preferences and still permits using the interface", async () => {
  vi.mocked(invoke).mockRejectedValue(new Error("Unreadable file"));
  render(<DesktopLayoutProvider><Controls /></DesktopLayoutProvider>);
  await userEvent.click(await screen.findByRole("button", { name: "Detalhes" }));
  expect(screen.getByText("details / general /")).toBeVisible();
  expect(vi.mocked(invoke).mock.calls.every(([command]) => command !== "save_desktop_layout")).toBe(true);
  expect(toast.error).toHaveBeenCalledWith("Não foi possível restaurar o layout");
});

it("waits for the previous save and retains both changes during a slow write", async () => {
  let release!: () => void;
  const firstSave = new Promise<void>(resolve => { release = resolve; });
  let writes = 0;
  vi.mocked(invoke).mockImplementation(command => {
    if (command === "get_desktop_layout") return Promise.resolve(DEFAULT_DESKTOP_LAYOUT);
    return ++writes === 1 ? firstSave : Promise.resolve();
  });
  render(<DesktopLayoutProvider><Controls /></DesktopLayoutProvider>);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Detalhes" }));
  await user.click(screen.getByRole("button", { name: "Skills" }));
  expect(writes).toBe(1);
  await act(async () => release());
  await waitFor(() => expect(writes).toBe(2));
  expect(invoke).toHaveBeenLastCalledWith("save_desktop_layout", { layout: expect.objectContaining({ inspectorTab: "details", settingsTab: "skills" }) });
});
