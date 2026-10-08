import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import { ProjectInstructionsReview } from "./ProjectInstructionsReview";

it("shows the complete proposed file and preserves the original for comparison", async () => {
  const user = userEvent.setup();
  const before = "# User rules\nNever publish automatically.\n";
  const after = `${before}\n# Project checks\nRun the existing tests.\n`;
  render(<ProjectInstructionsReview target={{ kind: "project_instructions", path: "AGENTS.md", before, after }} />);
  const proposed = screen.getByLabelText("Conteúdo proposto do AGENTS.md");
  expect(proposed).toHaveValue(after); expect(proposed).toHaveAttribute("readonly");
  await user.click(screen.getByRole("tab", { name: "Arquivo atual" }));
  expect(screen.getByLabelText("Conteúdo atual do AGENTS.md")).toHaveValue(before);
  expect(screen.getByLabelText("Conteúdo atual do AGENTS.md")).toHaveAttribute("readonly");
});

it("distinguishes a missing file from an existing empty file", async () => {
  const user = userEvent.setup();
  render(<ProjectInstructionsReview target={{ kind: "project_instructions", path: "AGENTS.md", before: "", after: "# Project rules" }} />);
  await user.click(screen.getByRole("tab", { name: "Arquivo atual" }));
  expect(screen.getByLabelText("Conteúdo atual do AGENTS.md")).toHaveValue("");
  expect(screen.queryByText("Este projeto ainda não possui AGENTS.md.")).not.toBeInTheDocument();
});
