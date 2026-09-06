import type { ProviderAccount } from "@/core/provider-accounts";

export function ProviderIcon({ kind, className = "size-3.5" }: { kind: ProviderAccount["providerKind"]; className?: string }) {
  return <span aria-hidden="true" className={`shrink-0 bg-current ${className}`} style={{ mask: `url('/provider-${kind === "antigravity" ? "antigravity" : "openai"}.svg') center / contain no-repeat` }} />;
}
