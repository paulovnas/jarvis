import { createElement, type ReactElement, type ReactNode } from "react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

type HintSide = "top" | "bottom" | "left" | "right" | "inline-start" | "inline-end";

export function Hint({ children, content, side = "top", className }: { children: ReactElement<{ children?: ReactNode }>; content: ReactNode; side?: HintSide; className?: string }) {
  if (content === null || content === undefined || content === "") return children;
  const { children: triggerChildren, ...triggerProps } = children.props;
  const trigger = createElement(children.type, triggerProps);
  return <Tooltip>
    <TooltipTrigger render={trigger}>{triggerChildren}</TooltipTrigger>
    <TooltipContent role="tooltip" side={side} className={cn("max-w-sm whitespace-pre-wrap break-words", className)}>{content}</TooltipContent>
  </Tooltip>;
}
