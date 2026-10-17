import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { ProviderUsageWindowSettings } from "./ProviderUsageWindowSettings";
import type { AccountUsage } from "@/core/provider-usage";

const { usage } = vi.hoisted(() => ({ usage: vi.fn() }));
vi.mock("@/hooks/use-provider-usage", () => ({ useProviderUsage: usage }));

it("offers Hyper window controls only for periods actually reported by the provider", async () => {
  const user = userEvent.setup(), onChange = vi.fn();
  const data: AccountUsage = { alias: "charm-hyper-pessoal", fetchedAt: 1, email: null, plan: null, creditBalance: 173, error: null, resetCredits: null, windows: [] };
  usage.mockReturnValue({ data, error: false });
  const props = { accountAlias: data.alias, providerName: "Meu Hyper", disabled: false, onChange };
  const view = render(<ProviderUsageWindowSettings {...props} />);
  expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  usage.mockReturnValue({ data: { ...data, windows: [{ id: "weekly", group: "Charm Hyper", label: "Semanal", durationSeconds: 604_800, remainingPercent: 70, resetsAt: 1_800_000_000_000, thirdParty: false }] }, error: false });
  view.rerender(<ProviderUsageWindowSettings {...props} />);
  expect(screen.queryByRole("checkbox", { name: "5 horas" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("checkbox", { name: "Semanal" }));
  expect(onChange).toHaveBeenCalledWith(true, false);
});

it("defaults to both windows and lets either or both be hidden independently", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  const view = render(<ProviderUsageWindowSettings providerName="Conta" disabled={false} onChange={onChange} />);
  const fiveHour = screen.getByRole("checkbox", { name: "5 horas" });
  const weekly = screen.getByRole("checkbox", { name: "Semanal" });
  expect(fiveHour).toBeChecked();
  expect(weekly).toBeChecked();
  await user.click(fiveHour);
  expect(onChange).toHaveBeenLastCalledWith(false, true);
  view.rerender(<ProviderUsageWindowSettings providerName="Conta" disabled={false} showFiveHourUsage={false} onChange={onChange} />);
  await user.click(weekly);
  expect(onChange).toHaveBeenLastCalledWith(false, false);
  view.rerender(<ProviderUsageWindowSettings providerName="Conta" disabled={false} showFiveHourUsage={false} showWeeklyUsage={false} onChange={onChange} />);
  await user.click(screen.getByText("5 horas"));
  expect(onChange).toHaveBeenLastCalledWith(true, false);
});

it("disables both controls without changing saved choices", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<ProviderUsageWindowSettings providerName="Conta" disabled showFiveHourUsage={false} showWeeklyUsage onChange={onChange} />);
  const fiveHour = screen.getByRole("checkbox", { name: "5 horas" });
  const weekly = screen.getByRole("checkbox", { name: "Semanal" });
  expect(fiveHour).toHaveAttribute("aria-disabled", "true");
  expect(fiveHour).not.toBeChecked();
  expect(weekly).toHaveAttribute("aria-disabled", "true");
  expect(weekly).toBeChecked();
  await user.click(screen.getByText("Semanal"));
  expect(onChange).not.toHaveBeenCalled();
});
