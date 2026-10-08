import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { McpProposalFields } from "./McpProposalFields";

it("masks credential values and prevents editing while an MCP decision is pending", () => {
  render(<McpProposalFields server={{ name: "docs", transport: "http", command: null, args: [], url: "https://example.com/mcp", cwd: null, enabled: true, envKeys: [], headerKeys: ["Authorization"] }} values={{ environment: {}, headers: { Authorization: "Bearer test-only-secret" } }} disabled onChange={vi.fn()} />);
  const field = screen.getByLabelText("Cabeçalho Authorization");
  expect(field).toHaveAttribute("type", "password");
  expect(field).toHaveValue("Bearer test-only-secret");
  expect(field).toBeDisabled();
  expect(screen.queryByText("Bearer test-only-secret")).not.toBeInTheDocument();
});
