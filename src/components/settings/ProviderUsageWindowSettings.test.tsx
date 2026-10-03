import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { ProviderUsageWindowSettings } from "./ProviderUsageWindowSettings";

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
