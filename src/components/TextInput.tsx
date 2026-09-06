import type { ComponentProps } from "react";
import { Input as RegistryInput } from "@/components/ui/input";
import { Textarea as RegistryTextarea } from "@/components/ui/textarea";
import { InputGroupInput as RegistryInputGroupInput } from "@/components/ui/input-group";
import { CommandInput as RegistryCommandInput } from "@/components/ui/command";

// Application defaults keep identifiers, paths, searches and configuration literal.
const literalText = { spellCheck: false, autoCorrect: "off", autoCapitalize: "off" } as const;

export function Input(props: ComponentProps<typeof RegistryInput>) {
  return <RegistryInput {...literalText} {...props} />;
}

export function Textarea(props: ComponentProps<typeof RegistryTextarea>) {
  return <RegistryTextarea {...literalText} {...props} />;
}

export function InputGroupInput(props: ComponentProps<typeof RegistryInputGroupInput>) {
  return <RegistryInputGroupInput {...literalText} {...props} />;
}

export function CommandInput(props: ComponentProps<typeof RegistryCommandInput>) {
  return <RegistryCommandInput {...literalText} {...props} />;
}
