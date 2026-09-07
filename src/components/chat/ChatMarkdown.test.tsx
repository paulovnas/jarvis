import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import ChatMarkdown from "./ChatMarkdown";

it("highlights Go and copies only code, preserving indentation", async () => {
  const user = userEvent.setup();
  const copy = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  const code = 'package main\n\nfunc main() {\n  println("Olá")\n}';
  render(<ChatMarkdown content={`Exemplo:\n\n\`\`\`go\n${code}\n\`\`\``} />);
  const block = screen.getByRole("region", { name: "Código go" });
  expect(block.querySelector(".hljs-keyword")).toHaveTextContent("package");
  await user.click(screen.getByRole("button", { name: "Copiar código" }));
  expect(copy).toHaveBeenCalledWith(code);
  expect(screen.getByText("Copiado")).toBeInTheDocument();
});
it("keeps unknown languages and HTML inert, with no copy control on inline code", () => {
  render(<ChatMarkdown content={'Use `go run`.\n\n```unknown\n<script>alert(1)</script>\n```'} />);
  expect(screen.getAllByRole("button", { name: "Copiar código" })).toHaveLength(1);
  expect(screen.getByText("<script>alert(1)</script>")).toBeInTheDocument();
  expect(document.querySelector("script")).toBeNull();
});
