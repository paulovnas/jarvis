import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

const logos = {
  icon: "/logo_icon.png",
  horizontal: "/logo_horizontal.png",
  vertical: "/logo_vertical.png",
} as const;

/** Text-bearing variants replace the wordmark instead of repeating it. */
export function JarvisLogo({ variant = "icon", className = "size-6", alt = "", ...props }: Omit<ComponentProps<"img">, "src"> & { variant?: keyof typeof logos }) {
  return <img src={logos[variant]} alt={alt} draggable={false} className={cn("object-contain", className)} {...props} />;
}
