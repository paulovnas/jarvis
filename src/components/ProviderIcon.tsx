import type { ProviderAccount } from "@/core/provider-accounts";
import { PlugZap, Sparkles, Zap } from "lucide-react";

export function ProviderIcon({ kind, className = "size-3.5" }: { kind: ProviderAccount["providerKind"]; className?: string }) {
  if (kind === "custom") return <PlugZap aria-hidden="true" className={`shrink-0 ${className}`} />;
  if (kind === "muse-code") return <Sparkles aria-hidden="true" className={`shrink-0 ${className}`} />;
  if (kind === "charm-hyper") return <Zap aria-hidden="true" className={`shrink-0 ${className}`} />;
  return <span aria-hidden="true" className={`shrink-0 bg-current ${className}`} style={{ mask: `url('/provider-${kind === "claude-code" ? "claude" : kind === "antigravity" ? "antigravity" : kind === "opencode-go" ? "opencode-go" : "openai"}.svg') center / contain no-repeat` }} />;
}
