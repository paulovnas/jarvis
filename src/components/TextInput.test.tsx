import { createRef } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { Command } from "@/components/ui/command";
import { InputGroup } from "@/components/ui/input-group";
import { CommandInput, Input, InputGroupInput, Textarea } from "./TextInput";

it("keeps technical fields literal, including grouped inputs and portal search controls", async () => {
  const user = userEvent.setup(); const changed = vi.fn(); const ref = createRef<HTMLInputElement>();
  render(<><Input aria-label="Alias" ref={ref} onChange={changed} /><Textarea aria-label="JSON" /><InputGroup><InputGroupInput aria-label="Sufixo" /></InputGroup><Command><CommandInput aria-label="Buscar" /></Command></>);
  for (const label of ["Alias", "JSON", "Sufixo", "Buscar"]) {
    const input = screen.getByLabelText(label);
    expect(input).toHaveAttribute("spellcheck", "false");
    expect(input).toHaveAttribute("autocorrect", "off");
    expect(input).toHaveAttribute("autocapitalize", "off");
  }
  await user.type(screen.getByLabelText("Alias"), "openrouter");
  expect(ref.current).toBe(screen.getByLabelText("Alias"));
  expect(ref.current).toHaveValue("openrouter");
  expect(changed).toHaveBeenCalled();
});

it("allows explicit prose fields to keep correction enabled", () => {
  render(<Textarea aria-label="Comentário" spellCheck autoCorrect="on" autoCapitalize="sentences" />);
  const input = screen.getByLabelText("Comentário");
  expect(input).toHaveAttribute("spellcheck", "true");
  expect(input).toHaveAttribute("autocorrect", "on");
  expect(input).toHaveAttribute("autocapitalize", "sentences");
});
