import { PlugZap } from "lucide-react";
import type { CSSProperties } from "react";
import type { ProviderAccount } from "@/core/provider-accounts";
import { cn } from "@/lib/utils";
import antigravityIcon from "../../public/provider-antigravity.svg?url";
import claudeIcon from "../../public/provider-claude.svg?url";
import openaiIcon from "../../public/provider-openai.svg?url";
import openCodeGoIcon from "../../public/provider-opencode-go.svg?url";

/** Bundle provider marks because the remote server serves only explicit asset paths. */
export function RemoteProviderIcon({ kind, className }: { kind: ProviderAccount["providerKind"]; className?: string }) {
  const classes = cn("size-4 shrink-0", className);
  if (kind === "custom") return <PlugZap aria-hidden="true" className={classes} />;
  const url = kind === "claude-code" ? claudeIcon : kind === "antigravity" ? antigravityIcon : kind === "opencode-go" ? openCodeGoIcon : openaiIcon;
  return <span aria-hidden="true" className={cn(classes, "bg-current [mask-image:var(--remote-provider-icon)] [mask-position:center] [mask-size:contain] [mask-repeat:no-repeat]")} style={{ "--remote-provider-icon": `url(${JSON.stringify(url)})` } as CSSProperties} />;
}
