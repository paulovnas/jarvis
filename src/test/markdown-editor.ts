import { screen, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";

export async function markdownSource(user: UserEvent, label: string) {
  const modes = await screen.findByRole("tablist", { name: `Modo de edição: ${label}` });
  await user.click(within(modes).getByRole("tab", { name: "Código" }));
  return screen.findByRole("textbox", { name: label });
}
