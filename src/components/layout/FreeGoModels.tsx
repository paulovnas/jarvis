import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { useOpencodeFreeModels } from "@/hooks/use-opencode-free-models";

export function FreeGoModels() {
  const { models, error } = useOpencodeFreeModels();
  return <section aria-label="Modelos gratuitos agora" className="mt-4 flex min-w-0 flex-col gap-2">
    <Separator className="mb-1" />
    <h3 className="micro-label text-muted-foreground">Gratuitos agora</h3>
    {error ? <p role="status" className="text-[11px] text-muted-foreground">Não foi possível confirmar os gratuitos agora.</p>
      : models === null ? <div role="status" aria-label="Consultando modelos gratuitos" className="flex flex-col gap-2"><Skeleton className="h-5" /><Skeleton className="h-5" /></div>
        : models.length === 0 ? <p className="text-[11px] text-muted-foreground">Nenhum modelo gratuito no momento.</p>
          : <ul className="flex flex-col gap-2">{models.map(model => <li key={model.id} className="flex min-w-0 items-center justify-between gap-3">
            <span className="min-w-0 break-words text-[11px]">{model.name}</span>
            <Badge variant="outline" className="border-onedark-green/30 bg-onedark-green/10 text-onedark-green">Grátis</Badge>
          </li>)}</ul>}
  </section>;
}
