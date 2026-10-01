import { act, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({ label: "main", roots: [] as { unmount(): void }[] }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ label: state.label }) }));
vi.mock("./core/webview-shortcuts", () => ({ installWebviewShortcutGuards: vi.fn() }));
vi.mock("./App", () => ({ default: () => <main>Aplicativo principal</main> }));
vi.mock("./components/companion/Companion", () => ({ Companion: () => <main>Assistente isolado</main> }));
vi.mock("react-dom/client", async original => {
  const actual = await original<typeof import("react-dom/client")>();
  return { ...actual, default: { ...actual, createRoot: (...args: Parameters<typeof actual.createRoot>) => {
    const root = actual.createRoot(...args); state.roots.push(root); return root;
  } } };
});
const tauriMarker = Object.getOwnPropertyDescriptor(window, "__TAURI_INTERNALS__");
const dark = document.documentElement.classList.contains("dark");

describe("Desktop window entry", () => {
  beforeEach(() => {
    vi.resetModules();
    const root = document.createElement("div"); root.id = "root"; document.body.append(root);
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
  });
  afterEach(() => {
    act(() => { state.roots.splice(0).forEach(root => root.unmount()); });
    document.getElementById("root")?.remove();
    delete document.documentElement.dataset.companion;
    document.documentElement.classList.toggle("dark", dark);
    if (tauriMarker) Object.defineProperty(window, "__TAURI_INTERNALS__", tauriMarker);
    else Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  });
  it("mounts only the companion and scopes transparency to its own window", async () => {
    state.label = "companion";
    await act(async () => { await import("./main"); });
    expect(await screen.findByText("Assistente isolado")).toBeVisible();
    expect(screen.queryByText("Aplicativo principal")).not.toBeInTheDocument();
    expect(document.documentElement).toHaveAttribute("data-companion");
  });
  it("keeps the normal application on the main window", async () => {
    state.label = "main";
    await act(async () => { await import("./main"); });
    expect(await screen.findByText("Aplicativo principal")).toBeVisible();
    expect(screen.queryByText("Assistente isolado")).not.toBeInTheDocument();
    expect(document.documentElement).not.toHaveAttribute("data-companion");
  });
});
