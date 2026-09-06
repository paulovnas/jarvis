import { ArchiveRestore } from "lucide-react";
import { Separator } from "@/components/ui/separator";
import type { CompactionEvent } from "@/core/chat";

export function CompactionMarker({ event }: { event: CompactionEvent }) {
  const date = new Date(event.createdAt);
  const count = new Intl.NumberFormat("pt-BR", { notation: "compact", maximumFractionDigits: 1 });
  return <div role="note" aria-label={event.automatic ? "Compactação automática concluída" : "Compactação manual concluída"} className="my-5 flex items-center gap-3 text-muted-foreground">
    <Separator className="flex-1" />
    <div className="flex flex-wrap items-center justify-center gap-x-2 gap-y-1 text-[10px]" title={`${date.toLocaleString("pt-BR")} · Tokens estimados: ${event.tokensBefore.toLocaleString("pt-BR")} → ${event.tokensAfter.toLocaleString("pt-BR")}`}>
      <ArchiveRestore aria-hidden="true" className="size-3 text-[#56b6c2]" />
      <span>Contexto compactado{event.automatic ? " · automático" : ""}</span>
      <span className="font-mono tabular-nums">~{count.format(event.tokensBefore)} → ~{count.format(event.tokensAfter)}</span>
      <time className="font-mono tabular-nums opacity-70" dateTime={date.toISOString()}>{date.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}</time>
    </div>
    <Separator className="flex-1" />
  </div>;
}
