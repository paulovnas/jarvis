import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { conversationContext } from "@/core/inspector";
import { ContextUsage } from "./ContextUsage";

const context = conversationContext([]);
const live = { tokens: 400, limit: 1000, estimated: false, compacting: false, compactions: 0 };

describe("Context meter", () => {
  it("shows useful metrics without explanatory copy and moves from green to red", () => {
    const { rerender } = render(<ContextUsage context={context} live={{ ...live, tokens: 0 }} />);
    const footer = screen.getByRole("contentinfo");
    expect(within(footer).getByText("0%")).toHaveStyle({ color: "rgb(152, 195, 121)" });
    expect(within(footer).queryByText(/medição|Compactação automática|catálogo/)).not.toBeInTheDocument();
    rerender(<ContextUsage context={context} live={{ ...live, tokens: 1000 }} />);
    expect(within(footer).getByText("100%")).toHaveStyle({ color: "rgb(224, 108, 117)" });
    expect(within(footer).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "100");
  });

  it("requires confirmation and locks the control while compacting", async () => {
    const user = userEvent.setup(); let resolve!: (value: boolean) => void;
    const compact = vi.fn(() => new Promise<boolean>(done => { resolve = done; }));
    render(<ContextUsage context={context} live={live} onCompact={compact} />);
    const button = screen.getByRole("button", { name: "Compactar contexto" });
    await user.click(button);
    expect(compact).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(compact).not.toHaveBeenCalled();
    await user.click(button);
    await waitFor(() => expect(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Compactar" })).toHaveFocus());
    await user.keyboard("{Enter}");
    expect(compact).toHaveBeenCalledTimes(1);
    expect(button).toBeDisabled();
    expect(screen.getByRole("contentinfo")).toHaveAttribute("data-compacting", "true");
    await act(async () => resolve(true));
    expect(button).toBeEnabled();
    expect(screen.getByRole("contentinfo")).not.toHaveAttribute("data-compacting");
  });

  it("shows the same busy state for automatic compaction and disables overlapping requests", () => {
    render(<ContextUsage context={context} live={{ ...live, compacting: true }} onCompact={vi.fn()} />);
    expect(screen.getByRole("contentinfo")).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("button", { name: "Compactar contexto" })).toBeDisabled();
  });
});
