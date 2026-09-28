import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { ChatBehaviorSettings } from "./ChatBehaviorSettings";
import type { TurnOptions } from "@/core/chat";

it("keeps automation optional and authorizes only selected actions, with Push required by PR", async () => {
  const user = userEvent.setup();
  const change = vi.fn();
  function Example() {
    const [publication, setPublication] = useState<TurnOptions["automaticPublication"] | null>(null);
    return <ChatBehaviorSettings manualAvailable manualValidation={false} onManualChange={vi.fn()} publication={publication ?? null} disabled={false} githubSelected={false} onPublicationChange={value => { change(value); setPublication(value); }} />;
  }
  render(<Example />);
  expect(screen.queryByRole("switch")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Configurações do chat" }));
  expect(screen.getByRole("switch", { name: "Github automático" })).not.toBeChecked();
  expect(screen.getByRole("switch", { name: "Validação manual" })).not.toBeChecked();
  await user.click(screen.getByRole("switch", { name: "Github automático" }));
  expect(change).toHaveBeenLastCalledWith({ commit: true, push: false, pullRequest: false });
  await user.click(screen.getByRole("switch", { name: "PR" }));
  expect(change).toHaveBeenLastCalledWith({ commit: true, push: true, pullRequest: true });
  await user.click(screen.getByRole("switch", { name: "Commit" }));
  expect(screen.getByRole("switch", { name: "Push" })).toHaveAttribute("aria-disabled", "true");
  await user.click(screen.getByRole("switch", { name: "Github automático" }));
  expect(change).toHaveBeenLastCalledWith(null);
  expect(screen.queryByRole("switch", { name: "PR" })).not.toBeInTheDocument();
});

it("hides manual validation in direct chats", async () => {
  const user = userEvent.setup();
  render(<ChatBehaviorSettings manualAvailable={false} manualValidation={false} onManualChange={vi.fn()} publication={null} onPublicationChange={vi.fn()} disabled={false} githubSelected={false} />);
  await user.click(screen.getByRole("button", { name: "Configurações do chat" }));
  expect(screen.queryByRole("switch", { name: "Validação manual" })).not.toBeInTheDocument();
  expect(screen.getByRole("switch", { name: "Github automático" })).not.toHaveAttribute("aria-disabled", "true");
});

it("removes the behavior menu and any open settings when Github is selected", async () => {
  const user = userEvent.setup();
  const props = { manualAvailable: true, manualValidation: true, onManualChange: vi.fn(), publication: { commit: true, push: true, pullRequest: true }, onPublicationChange: vi.fn(), disabled: false };
  const { rerender } = render(<ChatBehaviorSettings {...props} githubSelected={false} />);
  await user.click(screen.getByRole("button", { name: "Configurações do chat" }));
  expect(screen.getByRole("switch", { name: "Github automático" })).toBeChecked();
  rerender(<ChatBehaviorSettings {...props} githubSelected />);
  expect(screen.queryByRole("button", { name: "Configurações do chat" })).not.toBeInTheDocument();
  expect(screen.queryByRole("switch")).not.toBeInTheDocument();
});
