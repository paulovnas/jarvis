import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, Plus, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { Label } from "@/components/ui/label";
import { CardsSkeleton } from "@/components/layout/LoadingSkeletons";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { MCP_TEMPLATE, mcpCheckSchema, mcpServersSchema, validateMcpJson, type McpServer } from "@/core/mcp";
import { McpServerCard } from "./McpServerCard";

function message(error: unknown): string {
  if (typeof error === "object" && error !== null && "code" in error && error.code === "mcp_error" && "message" in error && typeof error.message === "string") return error.message;
  return "Não foi possível concluir a operação do MCP. Tente novamente.";
}

export function McpSettings({ onCountChange }: { onCountChange?: (count: number) => void }) {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [editor, setEditor] = useState<{ id: string | null } | null>(null);
  const [raw, setRaw] = useState("");
  const [editorError, setEditorError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<McpServer | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const pending = useRef(false);
  const mounted = useRef(false);
  const [checking, setChecking] = useState<ReadonlySet<string>>(() => new Set());
  const checkRequests = useRef(new Map<string, symbol>());
  const checkServer = useCallback(async (server: McpServer) => {
    if (!server.enabled || !server.configured) return;
    const request = Symbol(); checkRequests.current.set(server.id, request);
    if (mounted.current) setChecking(items => new Set(items).add(server.id));
    try {
      const check = mcpCheckSchema.parse(await invoke("test_mcp_server", { id: server.id }));
      if (mounted.current && checkRequests.current.get(server.id) === request) {
        setServers(items => items.map(item => item.id === server.id && item.revision === server.revision ? { ...item, lastCheck: check } : item));
        if (check.error) toast.error(check.error);
      }
    } catch (cause) {
      if (mounted.current && checkRequests.current.get(server.id) === request) {
        const error = message(cause);
        setServers(items => items.map(item => item.id === server.id && item.revision === server.revision ? { ...item, lastCheck: { toolCount: 0, tools: [], error } } : item));
        toast.error(error);
      }
    } finally {
      if (checkRequests.current.get(server.id) === request) {
        checkRequests.current.delete(server.id);
        if (mounted.current) setChecking(items => { const next = new Set(items); next.delete(server.id); return next; });
      }
    }
  }, []);

  useEffect(() => {
    let active = true;
    mounted.current = true;
    void invoke<unknown>("list_mcp_servers").then((value) => {
      if (active) { const parsed = mcpServersSchema.parse(value); setServers(parsed); onCountChange?.(parsed.length); setError(null); for (const server of parsed) { if (!server.lastCheck) void checkServer(server); } }
    }).catch(() => { if (active) setError("Não foi possível carregar os MCPs."); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; mounted.current = false; };
  }, [reload, onCountChange, checkServer]);

  async function perform(operation: () => Promise<void>, failed: (error: unknown) => void = (err) => toast.error(message(err))) {
    if (pending.current) return;
    pending.current = true; setBusy(true);
    try { await operation(); } catch (err) { if (mounted.current) failed(err); }
    finally { pending.current = false; if (mounted.current) setBusy(false); }
  }
  function update(value: unknown) { const parsed = mcpServersSchema.parse(value); if (mounted.current) { setServers(parsed); onCountChange?.(parsed.length); } }
  function closeEditor() { if (pending.current) return; setEditor(null); setRaw(""); setEditorError(null); }
  async function save() {
    if (!editor) return;
    const validation = validateMcpJson(raw);
    if (validation) { setEditorError(validation); return; }
    setEditorError(null);
    await perform(async () => {
      const saved = mcpServersSchema.parse(await invoke("save_mcp_server", { id: editor.id, config: raw }));
      update(saved);
      if (mounted.current) { setEditor(null); setRaw(""); toast.success("MCP salvo"); }
      const changed = saved.find(server => editor.id ? server.id === editor.id : !servers.some(previous => previous.id === server.id));
      if (changed) void checkServer(changed);
    }, (err) => setEditorError(message(err)));
  }

  return <div className="flex flex-col gap-4">
    <div className="flex items-start justify-between gap-3">
      <h2 className="text-sm font-medium">MCPs</h2>
      <div className="flex gap-1">
        <Button variant="ghost" size="icon-sm" aria-label="Atualizar MCPs" className="cursor-pointer" disabled={loading || busy} onClick={() => { setLoading(true); setReload((value) => value + 1); }}><RefreshCw aria-hidden="true" /></Button>
        <Button size="sm" className="cursor-pointer" disabled={loading || busy || error !== null} onClick={() => { setRaw(""); setEditorError(null); setEditor({ id: null }); }}><Plus aria-hidden="true" />Adicionar MCP</Button>
      </div>
    </div>
    {busy && <p role="status" className="flex items-center gap-2 text-xs text-muted-foreground"><Spinner aria-hidden="true" />Processando MCP…</p>}
    {loading ? <CardsSkeleton label="Carregando MCPs" columns /> : error ? <p role="alert" className="text-xs text-destructive">{error}</p> : servers.length === 0 ? <p className="rounded-lg border border-dashed p-6 text-center text-xs text-muted-foreground">Nenhum MCP cadastrado.</p> : <div className="grid items-start gap-3 sm:grid-cols-2">{servers.map((server) => <McpServerCard key={server.id} server={server} busy={busy || checking.has(server.id)} checking={checking.has(server.id)}
      onToggle={(enabled) => { void perform(async () => { update(await invoke("set_mcp_enabled", { id: server.id, enabled })); if (enabled) void checkServer({ ...server, enabled }); toast.success(enabled ? "MCP ativado" : "MCP desativado"); }); }}
      onEdit={() => { void perform(async () => { const value = await invoke<string>("get_mcp_config", { id: server.id }); if (mounted.current) { setRaw(value); setEditorError(null); setEditor({ id: server.id }); } }); }}
      onDelete={() => { setDeleteError(null); setDeleting(server); }}
      onTest={() => { void checkServer(server); }}
    />)}</div>}
    <Dialog open={editor !== null} onOpenChange={(open) => { if (!open) closeEditor(); }}>
      <DialogContent className="dark max-h-[85vh] overflow-auto sm:max-w-2xl">
        <DialogHeader><DialogTitle>{editor?.id ? "Editar MCP" : "Adicionar MCP"}</DialogTitle><DialogDescription>Configuração no formato OpenCode.</DialogDescription></DialogHeader>
        <Button variant="link" className="h-auto cursor-pointer justify-start p-0 text-xs" onClick={() => { void openUrl("https://opencode.ai/docs/mcp-servers/").catch(() => toast.error("Não foi possível abrir a documentação")); }}><ExternalLink aria-hidden="true" />Como configurar MCPs no OpenCode</Button>
        <form className="flex min-w-0 flex-col gap-3" onSubmit={(event) => { event.preventDefault(); void save(); }}>
          <Label htmlFor="mcp-json">Configuração JSON</Label>
          <Textarea id="mcp-json" value={raw} onChange={(event) => setRaw(event.currentTarget.value)} placeholder={MCP_TEMPLATE} disabled={busy} autoComplete="off" spellCheck={false} maxLength={65536} aria-invalid={editorError !== null} className="min-h-64 resize-y font-mono text-xs" />
          {editorError && <p role="alert" className="text-xs text-destructive">{editorError}</p>}
          <DialogFooter><Button type="button" variant="ghost" className="cursor-pointer" disabled={busy} onClick={closeEditor}>Cancelar</Button><Button type="submit" className="cursor-pointer" disabled={busy || !raw.trim()}>{busy && <Spinner aria-hidden="true" />}Salvar MCP</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
    <AlertDialog open={deleting !== null} onOpenChange={(open) => { if (!open && !pending.current) setDeleting(null); }}>
      <AlertDialogContent className="dark"><AlertDialogHeader><AlertDialogTitle>Excluir MCP {deleting?.name}?</AlertDialogTitle><AlertDialogDescription>A configuração e as credenciais deste MCP serão removidas definitivamente. Para manter os dados, desative o MCP.</AlertDialogDescription></AlertDialogHeader>
        {deleteError && <p role="alert" className="text-xs text-destructive">{deleteError}</p>}
        <AlertDialogFooter><AlertDialogCancel disabled={busy} className="cursor-pointer">Cancelar</AlertDialogCancel><AlertDialogAction disabled={busy} className="cursor-pointer bg-destructive text-destructive-foreground hover:bg-destructive/90" onClick={(event) => { event.preventDefault(); if (!deleting) return; void perform(async () => { update(await invoke("delete_mcp_server", { id: deleting.id })); if (mounted.current) setDeleting(null); toast.success("MCP excluído"); }, (err) => setDeleteError(message(err))); }}>Excluir MCP</AlertDialogAction></AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>;
}
