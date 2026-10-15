import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Hint } from "@/components/ui/hint";
import { useLibrary } from "@/hooks/use-library";

export function GlobalDirectorySettings() {
  const library = useLibrary();
  return <section aria-label="Pasta de trabalho global" className="mb-5 space-y-2">
    <Card size="sm" className="settings-group"><div className="settings-row">
      <div className="settings-row-copy"><h3 className="text-sm font-medium">Pasta de tarefas sem projeto</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">Pasta usada pelas conversas iniciadas fora de projetos.</p></div>
      <div className="settings-row-control flex max-w-full items-center gap-2">
        {library.loading ? <Skeleton aria-label="Carregando pasta global" className="h-4 w-40" /> : <Hint content={library.snapshot?.globalDirectory}><span className="max-w-60 truncate font-mono text-xs text-muted-foreground">{library.snapshot?.globalDirectory ?? "Pasta indisponível"}</span></Hint>}
        <Button variant="secondary" size="sm" className="shrink-0 cursor-pointer" disabled={library.loading || library.pending} onClick={() => void library.changeGlobalDirectory()}>{library.pending ? "Alterando…" : "Alterar"}</Button>
      </div>
    </div>
    </Card>
    {library.error && <p role="alert" className="text-xs text-destructive">{library.error}</p>}
  </section>;
}
