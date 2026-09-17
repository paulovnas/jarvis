import { createElement, useCallback, useLayoutEffect, useRef, useState, type ReactElement, type ReactNode } from "react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

type HintSide = "top" | "bottom" | "left" | "right" | "inline-start" | "inline-end";

type HintProps = {
  children: ReactElement<{ children?: ReactNode }>;
  content: ReactNode;
  side?: HintSide;
  className?: string;
  disabled?: boolean;
  whenTruncated?: boolean;
};

export function Hint({ children, content, side = "top", className, disabled = false, whenTruncated = false }: HintProps) {
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const [truncated, setTruncated] = useState(!whenTruncated);
  const measure = useCallback(() => {
    if (!whenTruncated) return;
    const element = triggerRef.current;
    setTruncated(Boolean(element && (element.scrollWidth > element.clientWidth || element.scrollHeight > element.clientHeight)));
  }, [whenTruncated]);
  useLayoutEffect(() => {
    if (!whenTruncated) {
      setTruncated(true);
      return;
    }
    measure();
    const element = triggerRef.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [content, measure, whenTruncated]);
  if (content === null || content === undefined || content === "") return children;
  const { children: triggerChildren, ...triggerProps } = children.props;
  const trigger = createElement(children.type, triggerProps);
  return <Tooltip disabled={disabled || (whenTruncated && !truncated)}>
    <TooltipTrigger ref={triggerRef} render={trigger} onPointerEnter={measure} onFocus={measure}>{triggerChildren}</TooltipTrigger>
    <TooltipContent role="tooltip" side={side} className={cn("max-w-sm whitespace-pre-wrap break-words", className)}>{content}</TooltipContent>
  </Tooltip>;
}
