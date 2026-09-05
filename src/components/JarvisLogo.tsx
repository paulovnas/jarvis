import type { ComponentProps } from "react";

/** A cut-corner J, drawn on a 32-unit grid to remain legible in tool headers. */
export function JarvisLogo({ className = "size-6", ...props }: ComponentProps<"svg">) {
  return <svg viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg" className={className} aria-hidden="true" {...props}>
    <path d="M4 4h11v3H7v8H4V4Zm13 21h8v-8h3v11H17v-3Z" fill="currentColor" opacity=".4" />
    <path d="M14 8h10v12l-5 5h-8l-4-4 3-3 3 3h4l3-3v-6h-6V8Z" fill="#61afef" />
    <path d="M14 8h10v1H14V8Z" fill="#d7edff" fillOpacity=".7" />
    <path d="M8 11h3v3H8v-3Z" fill="#56b6c2" />
  </svg>;
}
