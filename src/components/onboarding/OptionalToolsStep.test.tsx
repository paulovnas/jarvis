import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { OptionalToolsStep } from "./OptionalToolsStep";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const openUrlMock = vi.mocked(openUrl);

function snapshot(automaticInstall = true, gitInstalled = false) {
  return {
    platform: "windows",
    platformLabel: "Windows",
    tools: [
      { id: "git", name: "Git", description: "Versiona alterações", installed: gitInstalled, version: gitInstalled ? "git version 2.51.0" : null, automaticInstall, installWith: automaticInstall ? "WinGet" : null, helpUrl: "https://git-scm.com/download/win" },
      { id: "gh", name: "GitHub CLI", description: "Publica pull requests", installed: false, version: null, automaticInstall, installWith: automaticInstall ? "WinGet" : null, helpUrl: "https://cli.github.com/" },
    ],
  };
}

beforeEach(() => {
  invokeMock.mockReset(); openUrlMock.mockReset();
});

it("installs a missing tool with the detected operating-system package manager", async () => {
  invokeMock.mockResolvedValueOnce(snapshot()).mockResolvedValueOnce(snapshot(true, true));
  const user = userEvent.setup();
  render(<OptionalToolsStep onBusyChange={vi.fn()} />);
  await user.click(await screen.findByRole("button", { name: "Instalar Git com WinGet" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("install_optional_tool", { id: "git" }));
  expect(await screen.findByText("git version 2.51.0")).toBeVisible();
});

it("opens the operating-system instructions when automatic installation is unavailable", async () => {
  invokeMock.mockResolvedValue(snapshot(false)); openUrlMock.mockResolvedValue(undefined);
  const user = userEvent.setup();
  render(<OptionalToolsStep onBusyChange={vi.fn()} />);
  await user.click(await screen.findByRole("button", { name: "Abrir instruções de Git" }));
  expect(openUrlMock).toHaveBeenCalledWith("https://git-scm.com/download/win");
});
