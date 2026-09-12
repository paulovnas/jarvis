import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { WorkflowRecoveryAlert } from "./WorkflowRecoveryAlert";

describe("WorkflowRecoveryAlert", () => {
  it("highlights uncertain tools and starts an explicit resume", async () => {
    const user = userEvent.setup();
    const resume = vi.fn().mockResolvedValue(true);
    render(<WorkflowRecoveryAlert recovery={{
      runId: "turn-1",
      affectedAgents: 3,
      uncertainActions: [
        { agentId: "main", agentTitle: "Planejador", tool: "hub_wait" },
        { agentId: "worker", agentTitle: "Construtor", tool: "apply_patch" },
      ],
    }} onResume={resume} />);

    expect(screen.getByText(/3 agentes serão reconstruídos/)).toBeInTheDocument();
    expect(screen.getByLabelText("Ferramentas com resultado incerto")).toHaveTextContent("apply_patch");
    await user.click(screen.getByRole("button", { name: "Retomar fluxo" }));
    expect(resume).toHaveBeenCalledOnce();
  });

  it("explains the inspection gate when every prior result is durable", () => {
    render(<WorkflowRecoveryAlert recovery={{
      runId: "turn-2",
      affectedAgents: 1,
      uncertainActions: [],
    }} onResume={vi.fn().mockResolvedValue(true)} />);

    expect(screen.getByText(/exigirá uma leitura de verificação/)).toBeInTheDocument();
  });
});
