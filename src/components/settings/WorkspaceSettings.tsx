import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Folder, HardDrive, Layers, MessageSquare, RefreshCw, Trash2 } from "lucide-react";
import { z } from "zod";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { CardsSkeleton } from "@/components/layout/LoadingSkeletons";
import { DeleteItemDialog } from "@/components/layout/DeleteItemDialog";
import { libraryError } from "@/core/library";
import { Hint } from "@/components/ui/hint";

const identity = z.object({ id: z.string(), name: z.string() });
const storageSchema = z.array(z.object({ workspace: identity, conversations: z.number().nonnegative(), bytes: z.number().nonnegative(), projects: z.array(z.object({ project: identity.extend({ path: z.string() }), conversations: z.number().nonnegative(), bytes: z.number().nonnegative() })) }));
type Storage = z.infer<typeof storageSchema>[number];
function storageSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const power = Math.min(3, Math.floor(Math.log(bytes) / Math.log(1024)));
  return `${(bytes / 1024 ** power).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} ${["B", "KB", "MB", "GB"][power]}`;
}

export function WorkspaceSettings() {
  const [items, setItems] = useState<Storage[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [deletion, setDeletion] = useState<Storage | null>(null);
  const [pending, setPending] = useState(false);
  const generation = useRef(0);
  const load = useCallback(() => {
    const request = ++generation.current;
    return invoke("get_workspace_storage")
      .then(value => { const parsed = storageSchema.parse(value); if (generation.current === request) setItems(parsed); })
      .catch(cause => { if (generation.current === request) setError(libraryError(cause, "Não foi possível medir os históricos.")); })
      .finally(() => { if (generation.current === request) setLoading(false); });
  }, []);
  useEffect(() => { const requests = generation; void load(); const stop = listen("library:changed", () => void load()); return () => { requests.current++; void stop.then(dispose => dispose()).catch(() => {}); }; }, [load]);
  return <div className="space-y-4">
    <div className="flex items-center justify-between gap-3"><div><h2 className="micro-label text-muted-foreground">Histórico armazenado</h2><p className="mt-1 text-xs text-muted-foreground">Mensagens, anexos e memória das conversas. As pastas dos projetos não entram no cálculo.</p></div><Button variant="ghost" size="icon" aria-label="Atualizar armazenamento" disabled={loading || pending} onClick={() => { setLoading(true); setError(null); void load(); }}><RefreshCw className="size-4" /></Button></div>
    {error && !deletion && <p role="alert" className="text-sm text-destructive">{error}</p>}
    {loading ? <CardsSkeleton label="Medindo históricos dos workspaces" columns /> : <div className="grid gap-4 md:grid-cols-2">{items.map(item => <Card key={item.workspace.id} className="min-w-0 gap-4 bg-card">
      <CardHeader className="flex flex-row items-center gap-3"><Layers className="size-5 shrink-0 text-onedark-cyan" /><Hint content={item.workspace.name}><CardTitle className="min-w-0 flex-1 truncate text-sm">{item.workspace.name}</CardTitle></Hint><Button variant="ghost" size="icon" aria-label={`Excluir workspace ${item.workspace.name}`} className="shrink-0 text-muted-foreground hover:text-destructive" onClick={() => { setError(null); setDeletion(item); }}><Trash2 className="size-4" /></Button></CardHeader>
      <CardContent className="space-y-4"><div className="flex flex-wrap gap-4 font-mono text-xs text-muted-foreground"><span className="flex items-center gap-1.5"><Folder className="size-3.5" />{item.projects.length} projetos</span><span className="flex items-center gap-1.5"><MessageSquare className="size-3.5" />{item.conversations} conversas</span><Hint content="Espaço ocupado pelos históricos"><span className="flex items-center gap-1.5 text-onedark-cyan"><HardDrive className="size-3.5" />{storageSize(item.bytes)} em históricos</span></Hint></div>
        <div className="divide-y divide-border" aria-label={`Históricos dos projetos de ${item.workspace.name}`}>{item.projects.map(p => <div key={p.project.id} className="flex min-w-0 items-center gap-3 py-2.5 text-xs"><Hint content={p.project.name}><p className="min-w-0 flex-1 truncate font-medium">{p.project.name}</p></Hint><span className="shrink-0 font-mono text-[10px] text-muted-foreground">{p.conversations} conversas</span><Hint content={`Histórico armazenado por ${p.project.name}`}><span className="shrink-0 font-mono tabular-nums">{storageSize(p.bytes)}</span></Hint></div>)}</div>
      </CardContent>
    </Card>)}</div>}
    {!loading && !error && !items.length && <p className="py-8 text-center text-sm text-muted-foreground">Nenhum workspace cadastrado.</p>}
    {deletion && <DeleteItemDialog kind="workspace" name={deletion.workspace.name} conversationCount={deletion.conversations} pending={pending} error={error} onClose={() => { setDeletion(null); setError(null); }} onConfirm={async () => {
      setPending(true); setError(null);
      try { await invoke("delete_library_item", { target: { kind: "workspace", id: deletion.workspace.id }, confirmed: true }); toast.success("Workspace e históricos excluídos"); await load(); return true; }
      catch (cause) { setError(libraryError(cause, "Não foi possível excluir o workspace.")); return false; }
      finally { setPending(false); }
    }} />}
  </div>;
}
