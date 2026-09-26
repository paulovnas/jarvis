import type { ProviderAccount } from "@/core/provider-accounts";
import { PlugZap, TerminalSquare } from "lucide-react";

export function ProviderIcon({ kind, className = "size-3.5" }: { kind: ProviderAccount["providerKind"]; className?: string }) {
  if (kind === "claude-code") return <TerminalSquare aria-hidden="true" className={`shrink-0 ${className}`} />;
  if (kind === "custom") return <PlugZap aria-hidden="true" className={`shrink-0 ${className}`} />;
  return <span aria-hidden="true" className={`shrink-0 bg-current ${className}`} style={{ mask: `url('/provider-${kind === "antigravity" ? "antigravity" : "openai"}.svg') center / contain no-repeat` }} />;
}
