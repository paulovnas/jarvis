import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { z } from "zod";
import { pluginError } from "@/core/plugins";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Spinner } from "@/components/ui/spinner";

const statusSchema = z.object({ authenticated: z.boolean(), state: z.enum(["disconnected", "connecting", "connected", "error"]), error: z.string().nullable() });
const startSchema = z.object({ flowId: z.string(), authorizationUrl: z.string() });
type Status = z.infer<typeof statusSchema>;

export function PluginMcpAuth({ serverId, name, disabled, onBusyChange }: { serverId: string; name: string; disabled: boolean; onBusyChange: (busy: boolean) => void }) {
  const [status, setStatus] = useState<Status | null>(null); const [loading, setLoading] = useState(true); const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null); const [cancelling, setCancelling] = useState(false);
  const mounted = useRef(false); const pending = useRef(false); const flow = useRef<string | null>(null);
  useEffect(() => {
    let active = true; mounted.current = true;
    void invoke("mcp_oauth_status", { id: serverId }).then(raw => { if (active) setStatus(statusSchema.parse(raw)); }).catch(() => { if (active) setError("Não foi possível verificar a conexão deste MCP."); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; mounted.current = false; const flowId = flow.current; if (flowId) void invoke("cancel_mcp_oauth", { flowId }).catch(() => {}); };
  }, [serverId]);
  useEffect(() => { onBusyChange(busy); return () => onBusyChange(false); }, [busy, onBusyChange]);
  async function connect() {
    if (pending.current || disabled) return;
    pending.current = true; setBusy(true); setError(null);
    try {
      const started = startSchema.parse(await invoke("start_mcp_oauth", { id: serverId }));
      if (!mounted.current) { void invoke("cancel_mcp_oauth", { flowId: started.flowId }).catch(() => {}); return; }
      flow.current = started.flowId; setStatus({ authenticated: false, state: "connecting", error: null });
      await openUrl(started.authorizationUrl);
      const next = statusSchema.parse(await invoke("wait_mcp_oauth", { flowId: started.flowId }));
      if (mounted.current) { setStatus(next); setError(next.error); }
    } catch (cause) {
      const flowId = flow.current; if (flowId) await invoke("cancel_mcp_oauth", { flowId }).catch(() => {});
      if (mounted.current) { setStatus({ authenticated: false, state: "error", error: null }); setError(pluginError(cause)); }
    } finally { flow.current = null; pending.current = false; if (mounted.current) { setBusy(false); setCancelling(false); } }
  }
  async function cancel() {
    const flowId = flow.current; if (!flowId || cancelling) return;
    setCancelling(true);
    try { await invoke("cancel_mcp_oauth", { flowId }); }
    catch { if (mounted.current) { setError("Não foi possível cancelar a conexão. Tente novamente."); setCancelling(false); } }
  }
  async function disconnect() {
    if (pending.current || disabled) return;
    pending.current = true; setBusy(true); setError(null);
    try { const next = statusSchema.parse(await invoke("disconnect_mcp_oauth", { id: serverId })); if (mounted.current) setStatus(next); }
    catch (cause) { if (mounted.current) setError(pluginError(cause)); }
    finally { pending.current = false; if (mounted.current) setBusy(false); }
  }
  return <div className="mt-3 flex flex-col gap-2 border-t pt-3" aria-label={`Autenticação de ${name}`}>
    <div className="flex flex-wrap items-center justify-between gap-2"><Badge variant="outline">{loading ? "Verificando conexão" : busy && status?.state === "connecting" ? "Aguardando navegador" : status?.authenticated ? "Conectado" : "Conta não conectada"}</Badge>{busy && status?.state === "connecting" ? <Button variant="outline" size="sm" className="cursor-pointer" disabled={cancelling} onClick={() => { void cancel(); }}>Cancelar conexão</Button> : <Button variant="outline" size="sm" className="cursor-pointer" disabled={disabled || loading || busy} onClick={() => { if (status?.authenticated) void disconnect(); else void connect(); }}>{busy && <Spinner data-icon="inline-start" />}{status?.authenticated ? "Desconectar conta" : error ? "Tentar conectar novamente" : "Conectar conta"}</Button>}</div>
    <p className="text-xs text-muted-foreground">A autorização abre no navegador. Desconectar mantém a configuração do MCP.</p>
    {error && <p role="alert" className="text-xs text-destructive">{error}</p>}
  </div>;
}
