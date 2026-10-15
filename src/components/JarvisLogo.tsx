import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

const logos = {
  horizontal: "/v2/png/jarvis-horizontal@2x.png",
  mark: "/v2/svg/inset-mark.svg",
  symbol: "/v2/svg/inset-symbol.svg",
  icon: "/v2/svg/inset-icon.svg",
} as const;

export function JarvisLogo({ variant = "mark", className = "size-6", alt = "", ...props }: Omit<ComponentProps<"img">, "src"> & { variant?: keyof typeof logos }) {
  return <img src={logos[variant]} alt={alt} draggable={false} className={cn("object-contain", className)} {...props} />;
}
