import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { HookProposalFields } from "./HookProposalFields";

it("shows the saved command being deleted with its exact execution limits", () => {
  render(<HookProposalFields target={{ kind: "hook", after: null, before: {
    id: "a".repeat(32), name: "Contexto inicial", event: "SessionStart",
    command: 'node "scripts/contexto inicial.js"', matcher: "", timeoutSeconds: 60, enabled: true,
  } }} />);
  expect(screen.getByText('node "scripts/contexto inicial.js"')).toBeVisible();
  expect(screen.getByText("60 s")).toBeVisible();
  expect(screen.getByText("Sem filtro")).toBeVisible();
  expect(screen.getByText("Será removido")).toBeVisible();
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
});
