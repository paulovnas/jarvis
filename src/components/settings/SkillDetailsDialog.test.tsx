import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SkillDetailsDialog, type SkillSelection } from "./SkillDetailsDialog";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));

const call = vi.mocked(invoke);
const detail = (name: string) => ({ name, description: `${name} description`, content: `# ${name}\nInstructions`, path: null, source: "owner/repo", files: ["SKILL.md"] });

describe("SkillDetailsDialog", () => {
  beforeEach(() => { call.mockReset(); });

  it("shows a structural loading state until Marketplace details arrive", async () => {
    let resolveDetail: (value: unknown) => void = () => {};
    call.mockImplementation(() => new Promise(resolve => { resolveDetail = resolve; }));
    render(<SkillDetailsDialog selection={{ name: "senior-backend", source: "owner/repo", skillId: "senior-backend" }} onClose={vi.fn()} />);
    expect(await screen.findByRole("status", { name: "Carregando skill" })).toBeVisible();
    expect(screen.getByText("Carregando arquivos e instruções…")).toBeVisible();
    resolveDetail(detail("senior-backend"));
    expect(await screen.findByRole("heading", { name: "senior-backend", level: 1 })).toBeVisible();
  });

  it("discards a late response from the previously selected skill", async () => {
    let resolveFirst: (value: unknown) => void = () => {};
    let resolveSecond: (value: unknown) => void = () => {};
    call.mockImplementationOnce(() => new Promise(resolve => { resolveFirst = resolve; }))
      .mockImplementationOnce(() => new Promise(resolve => { resolveSecond = resolve; }));
    const first: SkillSelection = { name: "Primeira", source: "owner/repo", skillId: "first" };
    const second: SkillSelection = { name: "Segunda", source: "owner/repo", skillId: "second" };
    const view = render(<SkillDetailsDialog selection={first} onClose={vi.fn()} />);
    view.rerender(<SkillDetailsDialog selection={second} onClose={vi.fn()} />);
    resolveFirst(detail("Primeira"));
    await waitFor(() => expect(screen.queryByText("Instructions", { exact: false })).not.toBeInTheDocument());
    resolveSecond(detail("Segunda"));
    expect(await screen.findByRole("heading", { name: "Segunda", level: 1 })).toBeVisible();
  });

  it("offers a retry after a detail request fails", async () => {
    call.mockRejectedValueOnce({ code: "skill_error", message: "Falha temporária" }).mockResolvedValueOnce(detail("Skill"));
    const user = userEvent.setup();
    render(<SkillDetailsDialog selection={{ name: "Skill", id: "local-id" }} onClose={vi.fn()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Falha temporária");
    await user.click(screen.getByRole("button", { name: "Tentar novamente" }));
    expect(await screen.findByRole("heading", { name: "Skill", level: 1 })).toBeVisible();
  });
});
