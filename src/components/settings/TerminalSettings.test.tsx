import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SystemSnapshot } from "@/core/system-preferences";
import { TerminalSettings } from "./TerminalSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn() } }));

const call = vi.mocked(invoke);
const initial: SystemSnapshot = {
  preferences: {
    preventSleep: "active",
    notifications: true,
    askUserTimeoutSeconds: 45,
    terminal: { shell: null, arguments: [], fontFamily: null, fontSize: 13 },
  },
  sleepInhibited: true,
  sleepError: null,
  notificationError: null,
  availableTerminalShells: ["/bin/zsh", "/bin/bash"],
  resolvedTerminalShell: "/bin/zsh",
  terminalError: null,
};

describe("terminal preferences", () => {
  beforeEach(() => {
    vi.mocked(listen).mockReset().mockResolvedValue(vi.fn());
    vi.mocked(toast.success).mockReset();
    call.mockReset().mockImplementation(async (command, args) => {
      if (command === "get_system_preferences") return initial;
      if (command === "save_system_preferences") {
        const preferences = (args as { preferences: SystemSnapshot["preferences"] }).preferences;
        return { ...initial, preferences, resolvedTerminalShell: preferences.terminal.shell ?? "/bin/zsh" };
      }
      return undefined;
    });
  });

  it("configures shell argv and a Nerd Font while preserving other system preferences", async () => {
    const user = userEvent.setup();
    render(<TerminalSettings />);

    const shell = await screen.findByRole("combobox", { name: "Shell do terminal" });
    expect(shell).toHaveTextContent("Automático · zsh");
    await user.click(shell);
    await user.click(await screen.findByRole("option", { name: "bash · /bin/bash" }));

    await user.type(screen.getByRole("textbox", { name: "Argumentos de inicialização do shell" }), "-l\n-i");
    const font = screen.getByRole("combobox", { name: "Fonte do terminal" });
    await user.click(font);
    await user.click(await screen.findByRole("option", { name: "MesloLGS NF" }));
    const size = screen.getByRole("spinbutton", { name: "Tamanho da fonte do terminal" });
    await user.clear(size);
    await user.type(size, "15");
    await user.click(screen.getByRole("button", { name: "Salvar preferências" }));

    await waitFor(() => expect(call).toHaveBeenLastCalledWith("save_system_preferences", {
      preferences: {
        ...initial.preferences,
        terminal: { shell: "/bin/bash", arguments: ["-l", "-i"], fontFamily: "MesloLGS NF", fontSize: 15 },
      },
    }));
    expect(toast.success).toHaveBeenCalledWith("Preferências do terminal salvas", expect.objectContaining({ description: expect.stringContaining("terminais") }));
  });

  it("keeps a rejected custom shell visible so the user can correct it", async () => {
    const user = userEvent.setup();
    render(<TerminalSettings />);
    const shell = await screen.findByRole("combobox", { name: "Shell do terminal" });
    await user.click(shell);
    await user.click(await screen.findByRole("option", { name: "Personalizado…" }));
    const executable = screen.getByRole("textbox", { name: "Executável personalizado do shell" });
    await user.type(executable, "/shell/inexistente");
    call.mockRejectedValueOnce("O shell configurado não foi encontrado ou não pode ser executado: /shell/inexistente");
    await user.click(screen.getByRole("button", { name: "Salvar preferências" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("não foi encontrado");
    expect(executable).toHaveValue("/shell/inexistente");
    expect(screen.getByRole("button", { name: "Salvar preferências" })).toBeEnabled();
  });
});
