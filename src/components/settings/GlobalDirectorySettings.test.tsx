import { invoke } from "@tauri-apps/api/core";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { emptyLibrary } from "@/test/library-fixtures";
import { GlobalDirectorySettings } from "./GlobalDirectorySettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
beforeEach(() => vi.mocked(invoke).mockReset());

describe("Global working directory settings", () => {
  it("shows the current folder and updates it after the native picker", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async command => ({ ...emptyLibrary(), globalDirectory: command === "change_global_directory" ? "D:/Trabalho" : "C:/Users/Paulo/Jarvis" }));
    render(<GlobalDirectorySettings />);
    expect(await screen.findByText("C:/Users/Paulo/Jarvis")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Alterar" }));
    expect(await screen.findByText("D:/Trabalho")).toBeVisible();
    expect(invoke).toHaveBeenCalledWith("change_global_directory", {});
  });
  it("preserves the folder when selection is cancelled and reports failures", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockResolvedValueOnce({ ...emptyLibrary(), globalDirectory: "/home/paulo/Jarvis" }).mockResolvedValueOnce(null).mockRejectedValueOnce({ code: "active_conversation", message: "Interrompa as conversas globais." });
    render(<GlobalDirectorySettings />);
    expect(await screen.findByText("/home/paulo/Jarvis")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Alterar" }));
    expect(screen.getByText("/home/paulo/Jarvis")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Alterar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Interrompa as conversas globais.");
  });
});
