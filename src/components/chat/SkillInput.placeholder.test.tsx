import { readFileSync } from "node:fs";
import { render, screen } from "@testing-library/react";
import { afterAll, beforeAll, expect, it, vi } from "vitest";
import { SkillInput } from "./SkillInput";

const style = document.createElement("style");
beforeAll(() => {
  // jsdom cannot style pseudo-elements; project the same selectors onto a sibling probe.
  style.textContent = readFileSync("src/index.css", "utf8").replace(/::before/g, " + [data-placeholder-probe]");
  document.head.append(style);
});
afterAll(() => { style.remove(); });

it("keeps a clipped prediction visible above the empty editor without inserting draft text", () => {
  render(<SkillInput draft={{ content: "" }} prediction="Pode implementar os testes que você sugeriu?" onChange={vi.fn()} onSend={vi.fn()} disabled={false} compacting={false} working={false}>{null}</SkillInput>);
  const field = screen.getByRole("textbox", { name: "Mensagem" });
  const preview = document.createElement("span");
  preview.setAttribute("data-placeholder-probe", "");
  preview.setAttribute("aria-hidden", "true");
  field.after(preview);
  expect(field.textContent).toBe("");
  expect(field).toHaveAttribute("data-placeholder", "Pode implementar os testes que você sugeriu?");
  const computed = getComputedStyle(preview);
  expect(computed.overflow).toBe("hidden");
  expect(computed.height).not.toBe("0px");
});
