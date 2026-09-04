import type { ComponentProps } from "react";

export function JarvisLogo({ className = "size-6", ...props }: ComponentProps<"svg">) {
  return (
    <svg
      viewBox="0 0 1024 1024"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      aria-hidden="true"
      {...props}
    >
      <defs>
        <linearGradient id="jarvisChevronGrad" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="#56b6c2" />
          <stop offset="100%" stopColor="#61afef" />
        </linearGradient>
        <linearGradient id="jarvisJGrad" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%" stopColor="#61afef" />
          <stop offset="55%" stopColor="#56b6c2" />
          <stop offset="100%" stopColor="#98c379" />
        </linearGradient>
      </defs>

      {/* Terminal Code Prompt ">" */}
      <path
        d="M 270 380 L 390 490 L 270 600"
        stroke="url(#jarvisChevronGrad)"
        strokeWidth="76"
        strokeLinecap="round"
        strokeLinejoin="round"
      />

      {/* Terminal Cursor "_" */}
      <line
        x1="440"
        y1="600"
        x2="520"
        y2="600"
        stroke="#98c379"
        strokeWidth="54"
        strokeLinecap="round"
      />

      {/* The "J" backbone */}
      <path
        d="M 660 270 L 660 590 C 660 720 570 790 440 790 C 350 790 280 740 240 680"
        stroke="url(#jarvisJGrad)"
        strokeWidth="76"
        strokeLinecap="round"
        strokeLinejoin="round"
      />

      {/* AI Intelligence Spark indicator */}
      <circle cx="660" cy="270" r="30" fill="#61afef" fillOpacity="0.4" />
      <circle cx="660" cy="270" r="20" fill="#98c379" />
      <circle cx="660" cy="270" r="10" fill="#ffffff" />
    </svg>
  );
}
