import { useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { mcpCheckSchema, type McpServer } from "@/core/mcp";
import { readResource, ResourceTimeoutError } from "@/core/resource-request";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Spinner } from "@/components/ui/spinner";

type Props = { serverId: string | null; connectUrl: string | null; name: string; active: boolean; disabled: boolean; onBusyChange: (busy: boolean) => void };

function connectionError(cause: unknown): string {
  if (cause instanceof ResourceTimeoutError) return cause.message;
  if (typeof cause === "object" && cause !== null && "message" in cause && typeof cause.message === "string") return cause.message;
  return "Não foi possível testar a conexão deste app. Tente novamente.";
}

export function PluginAppsConnection({ serverId, connectUrl, name, active, disabled, onBusyChange }: Props) {
  const [check, setCheck] = useState<McpServer["lastCheck"]>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"authorize" | "test" | null>(null);
  const mounted = useRef(false); const pending = useRef(false);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => { onBusyChange(busy !== null); return () => onBusyChange(false); }, [busy, onBusyChange]);

  async function authorize() {
    if (pending.current || disabled || !connectUrl) return;
    pending.current = true; setBusy("authorize"); setError(null);
    try { await openUrl(connectUrl); if (mounted.current) toast.success("Autorização aberta no ChatGPT. Depois, teste a conexão do app."); }
    catch { if (mounted.current) setError("Não foi possível abrir a autorização no ChatGPT. Tente novamente."); }
    finally { pending.current = false; if (mounted.current) setBusy(null); }
  }

  async function test() {
    if (pending.current || disabled || !active || !serverId) return;
    pending.current = true; setBusy("test"); setError(null); setCheck(null);
    try {
      const result = mcpCheckSchema.parse(await readResource("test_mcp_server", { id: serverId }));
      if (mounted.current) { setCheck(result); setError(result.error); if (result.error) toast.error(result.error); }
    } catch (cause) { if (mounted.current) { const message = connectionError(cause); setError(message); toast.error(message); } }
    finally { pending.current = false; if (mounted.current) setBusy(null); }
  }

  const verified = active && check !== null && check.error === null && check.toolCount > 0;
  return <div className="mt-3 flex flex-col gap-2 border-t pt-3" aria-label={`Conexão de ${name}`}>
    <Badge variant="outline" className={verified ? "self-start text-onedark-green" : "self-start"}>{busy === "test" ? "Testando conexão" : verified ? "Gateway verificado" : error ? "Conexão não confirmada" : check?.toolCount === 0 ? "Nenhuma ferramenta disponível" : "Conexão ainda não verificada"}</Badge>
    <div className="flex flex-wrap gap-2"><Button variant="outline" size="sm" className="cursor-pointer" disabled={disabled || busy !== null || !connectUrl} onClick={() => { void authorize(); }}>{busy === "authorize" ? <Spinner data-icon="inline-start" /> : <ExternalLink data-icon="inline-start" />}Autorizar no ChatGPT</Button><Button variant="outline" size="sm" className="cursor-pointer" disabled={disabled || busy !== null || !active || !serverId} onClick={() => { void test(); }}>{busy === "test" ? <Spinner data-icon="inline-start" /> : <RefreshCw data-icon="inline-start" />}Testar conexão</Button></div>
    {!active && <p className="text-xs text-muted-foreground">Ative o plugin e este app e selecione uma conta ChatGPT disponível para testar a conexão.</p>}
    {verified && <><p className="font-mono text-xs text-onedark-green">{check.toolCount} {check.toolCount === 1 ? "ferramenta disponível" : "ferramentas disponíveis"} nos apps deste plugin.</p><p className="text-xs text-muted-foreground">A autorização de cada conector será confirmada ao usar suas ferramentas.</p></>}
    {check?.toolCount === 0 && check.error === null && <p className="text-xs text-muted-foreground">O gateway respondeu, mas não disponibilizou ferramentas dos apps deste plugin. Autorize o conector no ChatGPT e teste novamente.</p>}
    {error && <p role="alert" className="text-xs whitespace-pre-wrap text-destructive">{error}</p>}
  </div>;
}
