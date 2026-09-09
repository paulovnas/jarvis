import { invoke } from "@tauri-apps/api/core";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { BootstrapResourcesProvider } from "@/components/bootstrap/BootstrapResourcesProvider";
import type { BootstrapResources } from "@/core/bootstrap";
import type { SkillSnapshot } from "@/core/skills";
import { coreFixture } from "@/test/core-fixtures";
import { emptyLibrary } from "@/test/library-fixtures";
import { ResourceUpdates } from "./ResourceUpdates";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const coreEvents = new Map<string, EventCallback<unknown>>();

function skillSnapshot(updateAvailable: boolean): SkillSnapshot {
  return {
    includeAgents: false,
    directory: "/home/.jarvis/skills",
    warnings: [],
    skills: [{
      id: "jarvis/example",
      name: "example",
      description: "Example skill",
      origin: "jarvis",
      path: "/home/.jarvis/skills/example",
      removalPath: "/home/.jarvis/skills/example",
      linked: false,
      enabled: true,
      automatic: true,
      source: "owner/repository",
      marketplaceId: "owner/repository/example",
      updateAvailable,
      updateError: null,
    }],
  };
}

function resources(): BootstrapResources {
  const core = coreFixture();
  core.items[0] = { ...core.items[0], latestVersion: "1.1.0", updateAvailable: true };
  return {
    core,
    skills: skillSnapshot(true),
    accounts: [],
    usageByAlias: {},
    library: emptyLibrary(),
    checked: { core: true, skills: true },
    loaded: { core: true, skills: true, accounts: true, usage: true, library: true },
    warnings: [],
  };
}

describe("ResourceUpdates", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    coreEvents.clear();
    vi.mocked(listen).mockImplementation(async (name, callback) => {
      coreEvents.set(name, callback);
      return () => { coreEvents.delete(name); };
    });
  });

  it("lists Core and Marketplace updates and installs them in one action", async () => {
    const updatedCore = coreFixture();
    const updatedSkills = skillSnapshot(false);
    invokeMock.mockImplementation((command) => {
      if (command === "install_core_component") return Promise.resolve(updatedCore);
      if (command === "update_skills") return Promise.resolve({ snapshot: updatedSkills, updated: 1, errors: [] });
      return Promise.resolve([]);
    });
    const user = userEvent.setup();

    render(<BootstrapResourcesProvider initial={resources()}><ResourceUpdates /></BootstrapResourcesProvider>);

    await user.click(screen.getByRole("button", { name: "2 atualizações de recursos disponíveis" }));
    expect(screen.getByRole("heading", { name: "Atualizações de recursos" })).toBeVisible();
    expect(screen.getByText("Context-mode")).toBeVisible();
    expect(screen.getByText("example")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Instalar 2 atualizações" }));

    await waitFor(() => expect(screen.getByText("Todas as atualizações foram instaladas.")).toBeVisible());
    expect(invokeMock).toHaveBeenCalledWith("install_core_component", { id: "context-mode" });
    expect(invokeMock).toHaveBeenCalledWith("update_skills", { ids: ["jarvis/example"] });
  });

  it("stays hidden when every resource is current", () => {
    const current = resources();
    current.core = coreFixture();
    current.skills = skillSnapshot(false);

    render(<BootstrapResourcesProvider initial={current}><ResourceUpdates /></BootstrapResourcesProvider>);

    expect(screen.queryByText("Atualizações de recursos")).not.toBeInTheDocument();
  });

  it("shows indeterminate Core progress and downloaded MB when the total is unknown", async () => {
    const current = resources();
    current.skills = skillSnapshot(false);
    current.core = coreFixture();
    current.core.items[3] = {
      ...current.core.items[3],
      latestVersion: "next",
      updateAvailable: true,
    };
    const finished = coreFixture();
    let finishInstall!: (value: unknown) => void;
    invokeMock.mockImplementation(command => command === "install_core_component"
      ? new Promise(resolve => { finishInstall = resolve; })
      : Promise.resolve([]));
    const user = userEvent.setup();

    render(<BootstrapResourcesProvider initial={current}><ResourceUpdates /></BootstrapResourcesProvider>);
    await waitFor(() => expect(coreEvents.has("core:download")).toBe(true));
    await user.click(screen.getByRole("button", { name: "1 atualização de recurso disponível" }));
    await user.click(screen.getByRole("button", { name: "Instalar atualização" }));

    const downloading = structuredClone(current.core);
    downloading.items[3].stage = "Baixando recursos de design";
    await act(async () => coreEvents.get("core:changed")?.({
      event: "core:changed",
      id: 1,
      payload: downloading,
    }));
    await act(async () => coreEvents.get("core:download")?.({
      event: "core:download",
      id: 2,
      payload: {
        id: "open-design",
        download: { receivedBytes: 12 * 1024 * 1024, totalBytes: null },
      },
    }));

    const progress = screen.getByRole("progressbar", { name: "Atualização de Open Design" });
    expect(progress).toHaveAttribute("data-indeterminate");
    expect(progress).not.toHaveAttribute("aria-valuenow");
    expect(screen.getByText("12 MB")).toBeVisible();

    await act(async () => finishInstall(finished));
  });
});
