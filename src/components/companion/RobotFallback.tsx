import { useId } from "react";
import type { RobotProps } from "./Robot";

/** Static original artwork remains visible if the local runtime cannot load. */
export function RobotFallback({ status, gesture = "none", voiceLevel = 0, renderer = "fallback" }: Pick<RobotProps, "status" | "gesture" | "voiceLevel"> & { renderer?: "loading" | "fallback" }) {
  const id = useId().replace(/:/g, "");
  const sleeping = gesture === "sleep";
  return <svg aria-hidden="true" className="companion-robot" data-state={status} data-gesture={gesture} data-renderer={renderer} data-character="floating-robot" viewBox="0 0 120 120" fill="none">
    <defs>
      <linearGradient id={`${id}-shell`} x1="20" y1="35" x2="103" y2="112" gradientUnits="userSpaceOnUse"><stop stopColor="var(--foreground)" /><stop offset=".5" stopColor="var(--muted-foreground)" /><stop offset="1" stopColor="var(--secondary)" /></linearGradient>
      <linearGradient id={`${id}-glass`} x1="37" y1="56" x2="94" y2="111" gradientUnits="userSpaceOnUse"><stop stopColor="var(--secondary)" /><stop offset=".3" stopColor="var(--sidebar)" /><stop offset="1" stopColor="var(--background)" /></linearGradient>
      <radialGradient id={`${id}-light`}><stop stopColor="var(--foreground)" /><stop offset=".3" stopColor="var(--companion-light)" /><stop offset="1" stopColor="var(--companion-light)" stopOpacity=".35" /></radialGradient>
    </defs>
    <ellipse cx="60" cy="111" rx="27" ry="2" fill="var(--sidebar)" opacity=".2" />
    <rect x="13" y="59" width="9" height="23" rx="4.5" fill={`url(#${id}-shell)`} stroke="var(--border)" />
    <rect x="98" y="59" width="9" height="23" rx="4.5" fill={`url(#${id}-shell)`} stroke="var(--border)" />
    <path d="M60 34V27" stroke="var(--muted-foreground)" strokeWidth="4" strokeLinecap="round" />
    <circle cx="60" cy="25" r="3.5" fill={`url(#${id}-light)`} />
    <rect x="18" y="32" width="84" height="76" rx="27" fill={`url(#${id}-shell)`} stroke="var(--border)" strokeWidth="1.1" />
    <path d="M35 35h48" stroke="var(--foreground)" strokeOpacity=".18" strokeWidth="1.5" strokeLinecap="round" />
    <rect x="24" y="47" width="72" height="52" rx="19" fill={`url(#${id}-glass)`} stroke="var(--companion-light)" strokeOpacity=".5" />
    <path d="M38 52h44" stroke="var(--foreground)" strokeOpacity=".1" strokeWidth="2" strokeLinecap="round" />
    {status === "running" && <g stroke="var(--companion-light)" strokeWidth="2" strokeLinecap="round"><path d="M36 57l12-2" /><path d="M72 55l12 2" /></g>}
    <g fill="var(--companion-light)">
      {sleeping || gesture === "poke" || gesture === "sleepy" ? <path data-expression="closed-eyes" d="M36 71h12M72 71h12" stroke="var(--companion-light)" strokeWidth="3" strokeLinecap="round" /> : gesture === "dizzy" ? <g data-expression="dizzy" stroke="var(--companion-light)" strokeWidth="2" strokeLinecap="round" fill="none"><path d="M42 70c-4-4-7 3-3 5s9-2 6-7-11-4-12 2" /><path d="M78 70c-4-4-7 3-3 5s9-2 6-7-11-4-12 2" /></g> : status === "completed" ? <path d="M36 71q6-9 12 0M72 71q6-9 12 0" stroke="var(--companion-light)" strokeWidth="4" strokeLinecap="round" fill="none" /> : <><rect x="36" y={status === "failed" ? "67" : "62"} width="12" height={status === "failed" ? "8" : "16"} rx="6" /><rect x="72" y={status === "failed" ? "67" : "62"} width="12" height={status === "failed" ? "8" : "16"} rx="6" /></>}
    </g>
    {sleeping ? <g data-expression="sleep" stroke="var(--companion-light)" strokeWidth="2" strokeLinecap="round"><path d="M55 85q5 3 10 0" /><path d="M86 24h6l-6 7h6M98 10h9l-9 10h9" opacity=".65" /></g> : gesture === "speak" ? <ellipse data-expression="speaking" cx="60" cy="86" rx="5" ry={1 + Math.max(0, Math.min(1, voiceLevel)) * 5} fill="var(--companion-light)" /> : status === "waiting" || status === "reconnecting" ? <ellipse cx="60" cy="86" rx="3.5" ry="4.5" fill="var(--companion-light)" opacity=".85" /> : <path d={status === "failed" ? "M54 90q6-6 12 0" : status === "running" ? "M54 86q3-2 6 0t6 0" : "M53 85q7 7 14 0"} stroke="var(--companion-light)" strokeWidth="2.5" strokeLinecap="round" />}
    {gesture === "listen" && <path data-expression="listening" d="M36 56l12-2M72 54l12 2" stroke="var(--companion-light)" strokeWidth="2" strokeLinecap="round" />}
    {gesture === "curious" && <path data-expression="curious" d="M35 55l13-3M72 56h12" stroke="var(--companion-light)" strokeWidth="2" strokeLinecap="round" />}
    {gesture === "stretch" && <g data-expression="stretch" stroke={`url(#${id}-shell)`} strokeWidth="7" strokeLinecap="round"><path d="M19 79L9 42M101 79l10-37" /></g>}
    <path d="M54 104h12" stroke="var(--companion-light)" strokeWidth="2.5" strokeLinecap="round" opacity=".7" />
    <circle cx="85" cy="38" r="2" fill="var(--companion-light)" opacity=".7" />
    {status === "running" && <g data-gesture="thinking"><path d="M98 80l-13 10" stroke={`url(#${id}-shell)`} strokeWidth="7" strokeLinecap="round" /><rect x="72" y="85" width="14" height="11" rx="5.5" fill={`url(#${id}-shell)`} stroke="var(--border)" /></g>}
    {status === "waiting" && <g data-gesture="question"><path d="M20 80l-8 8" stroke={`url(#${id}-shell)`} strokeWidth="7" strokeLinecap="round" /><rect x="6" y="80" width="13" height="16" rx="6" fill={`url(#${id}-shell)`} stroke="var(--border)" /></g>}
  </svg>;
}
