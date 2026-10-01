import { Progress } from "@/components/ui/progress";
import { Button } from "@/components/ui/button";
import type { CoreSnapshot } from "@/core/core-components";

function megabytes(bytes: number) {
  return `${(bytes / 1024 / 1024).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} MB`;
}

export function CoreInstallProgress({
  item,
  operation = "Instalação",
  onCancel,
  cancelling = false,
}: {
  item: CoreSnapshot["items"][number];
  operation?: "Instalação" | "Atualização";
  onCancel?: () => void;
  cancelling?: boolean;
}) {
  const transfer = item.download;
  const percent = transfer?.totalBytes
    ? Math.min(100, Math.floor(transfer.receivedBytes / transfer.totalBytes * 100))
    : null;
  const amount = transfer
    ? `${megabytes(transfer.receivedBytes)}${transfer.totalBytes ? ` / ${megabytes(transfer.totalBytes)}` : ""}`
    : null;

  return <div className="mt-3 space-y-2 border-t border-border pt-3">
    <div className="flex items-start justify-between gap-2 text-[11px]">
      <span role="status" className="text-primary">{item.stage}</span>
      {percent !== null && <span className="font-mono tabular-nums text-primary">{percent}%</span>}
    </div>
    <Progress
      value={percent}
      aria-label={`${operation} de ${item.name}`}
      aria-valuetext={[item.stage, percent !== null ? `${percent}%` : null, amount].filter(Boolean).join(" · ")}
      className="core-install-progress [&_[data-slot=progress-track]]:h-1.5"
    />
    {amount && <p className="text-right font-mono text-[10px] tabular-nums text-muted-foreground">{amount}</p>}
    {onCancel && item.stage !== "Analisando componente" && <div className="flex justify-end"><Button type="button" variant="ghost" size="sm" disabled={cancelling} aria-label={`Cancelar ${operation.toLowerCase()} de ${item.name}`} onClick={onCancel} className="h-7 cursor-pointer text-[11px] text-muted-foreground">{cancelling ? "Cancelando…" : "Cancelar"}</Button></div>}
  </div>;
}
