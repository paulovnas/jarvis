import { useId } from "react";
import type { CompanionStatus } from "@/core/companion";

/** Static original artwork remains visible if the local runtime cannot load. */
export function RobotFallback({ status, renderer = "fallback" }: { status: CompanionStatus; renderer?: "loading" | "fallback" }) {
  const id = useId().replace(/:/g, "");
  return <svg aria-hidden="true" className="companion-robot" data-state={status} data-renderer={renderer} viewBox="0 0 120 140" fill="none">
    <defs>
      <linearGradient id={`${id}-shell`} x1="20" y1="35" x2="103" y2="122" gradientUnits="userSpaceOnUse"><stop stopColor="var(--secondary)" /><stop offset=".5" stopColor="var(--card)" /><stop offset="1" stopColor="var(--sidebar)" /></linearGradient>
      <linearGradient id={`${id}-glass`} x1="37" y1="56" x2="94" y2="111" gradientUnits="userSpaceOnUse"><stop stopColor="var(--secondary)" /><stop offset=".3" stopColor="var(--sidebar)" /><stop offset="1" stopColor="var(--background)" /></linearGradient>
      <radialGradient id={`${id}-light`}><stop stopColor="var(--foreground)" /><stop offset=".3" stopColor="var(--companion-light)" /><stop offset="1" stopColor="var(--companion-light)" stopOpacity=".35" /></radialGradient>
    </defs>
    <ellipse cx="60" cy="132" rx="29" ry="3" fill="var(--sidebar)" opacity=".25" />
    <rect x="40" y="107" width="16" height="24" rx="7" fill={`url(#${id}-shell)`} stroke="var(--border)" />
    <rect x="65" y="107" width="16" height="24" rx="7" fill={`url(#${id}-shell)`} stroke="var(--border)" />
    <path d="M38 104q22 15 44 0v10q-22 13-44 0Z" fill="var(--sidebar)" stroke="var(--border)" />
    <rect x="12" y="75" width="15" height="31" rx="7.5" fill={`url(#${id}-shell)`} stroke="var(--border)" /><path d="M19 91v8" stroke="var(--companion-light)" strokeWidth="2" strokeLinecap="round" opacity=".7" />
    <path d="M60 32V19" stroke="var(--secondary)" strokeWidth="6" strokeLinecap="round" />
    <circle cx="60" cy="15" r="6" fill={`url(#${id}-light)`} stroke="var(--companion-light)" strokeWidth="1.5" />
    <rect x="18" y="29" width="84" height="80" rx="25" fill={`url(#${id}-shell)`} stroke="var(--border)" strokeWidth="1.5" />
    <path d="M35 35h48" stroke="var(--foreground)" strokeOpacity=".18" strokeWidth="1.5" strokeLinecap="round" />
    <rect x="25" y="47" width="70" height="52" rx="18" fill={`url(#${id}-glass)`} stroke="var(--companion-light)" strokeOpacity=".5" />
    <path d="M38 52h44" stroke="var(--foreground)" strokeOpacity=".1" strokeWidth="2" strokeLinecap="round" />
    {status === "running" && <g stroke="var(--companion-light)" strokeWidth="2" strokeLinecap="round"><path d="M36 57l12-2" /><path d="M72 55l12 2" /></g>}
    <g fill="var(--companion-light)">
      {status === "completed" ? <path d="M36 71q6-9 12 0M72 71q6-9 12 0" stroke="var(--companion-light)" strokeWidth="4" strokeLinecap="round" fill="none" /> : <><rect x="36" y={status === "failed" ? "67" : "62"} width="12" height={status === "failed" ? "8" : "16"} rx="6" /><rect x="72" y={status === "failed" ? "67" : "62"} width="12" height={status === "failed" ? "8" : "16"} rx="6" /></>}
    </g>
    {status === "waiting" || status === "reconnecting" ? <ellipse cx="60" cy="86" rx="3.5" ry="4.5" fill="var(--companion-light)" opacity=".85" /> : <path d={status === "failed" ? "M54 90q6-6 12 0" : status === "running" ? "M54 86q3-2 6 0t6 0" : "M53 85q7 7 14 0"} stroke="var(--companion-light)" strokeWidth="2.5" strokeLinecap="round" />}
    <path d="M54 104h12" stroke="var(--companion-light)" strokeWidth="2.5" strokeLinecap="round" opacity=".7" />
    <circle cx="85" cy="38" r="2" fill="var(--companion-light)" opacity=".7" />
    <rect x="93" y="75" width="15" height="31" rx="7.5" fill={`url(#${id}-shell)`} stroke="var(--border)" /><path d="M101 91v8" stroke="var(--companion-light)" strokeWidth="2" strokeLinecap="round" opacity=".7" />
  </svg>;
}
