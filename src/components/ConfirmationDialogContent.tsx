import { useRef, type ComponentProps } from "react";
import { AlertDialogContent } from "@/components/ui/alert-dialog";

/** Focus the declared action so Enter confirms, while Tab and Escape stay native. */
export function ConfirmationDialogContent({ onKeyDown, ...props }: Omit<ComponentProps<typeof AlertDialogContent>, "ref" | "initialFocus">) {
  const content = useRef<HTMLDivElement>(null);
  return <AlertDialogContent
    {...props}
    ref={content}
    initialFocus={() => content.current?.querySelector<HTMLButtonElement>(":is([data-confirm-action], [data-slot=alert-dialog-action]):not(:disabled):not([aria-disabled=true])") ?? content.current}
    onKeyDown={event => {
      onKeyDown?.(event);
      if (event.key === "Enter" && (event.repeat || event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229)) event.preventDefault();
    }}
  />;
}
