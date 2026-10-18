import { readFileSync } from "node:fs";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterAll, beforeAll, expect, it, vi } from "vitest";
import { MarkdownEditor } from "@/components/MarkdownEditor";

const style = document.createElement("style");
beforeAll(() => {
  // jsdom cannot style native parts or evaluate feature queries; project their rules onto probes.
  const rules = readFileSync("src/styles/scrollbars.css", "utf8").match(/[^{}]*::-webkit-scrollbar[^{}]*\{[^{}]*\}/g) ?? [];
  style.textContent = rules.join("\n")
    .replace(/::-webkit-scrollbar(-[\w-]+)?/g, (_, part: string | undefined) => `[data-scrollbar-part="${part ?? "bar"}"]`);
  document.head.append(style);
});
afterAll(() => { style.remove(); });

it.each(["Editor", "Código"])("uses the arrow over the %s scrollbar while keeping the text cursor in its content", async mode => {
  const user = userEvent.setup();
  render(<div style={{ cursor: "text" }}><MarkdownEditor label="Conhecimento" value="Documento editável" onChange={vi.fn()} /></div>);
  if (mode === "Código") await user.click(screen.getByRole("tab", { name: mode }));
  const field = screen.getByRole("textbox", { name: "Conhecimento" });
  expect(getComputedStyle(field).cursor).toBe("text");
  for (const part of ["bar", "-track", "-thumb", "-corner"]) {
    const probe = document.createElement("span");
    probe.setAttribute("data-scrollbar-part", part);
    probe.setAttribute("aria-hidden", "true");
    field.after(probe);
    expect(getComputedStyle(probe).cursor).toBe("default");
  }
});
