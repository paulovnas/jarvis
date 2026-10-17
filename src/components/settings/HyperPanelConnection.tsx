import { useEffect, useId, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertCircle } from "lucide-react";
import { toast } from "sonner";
import { z } from "zod";
import { RowsSkeleton } from "@/components/layout/LoadingSkeletons";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useBootstrapResources } from "@/hooks/use-bootstrap-resources";

const panelStatusSchema = z.object({
  connected: z.boolean(),
  teamName: z.string().nullable(),
  teamId: z.string().min(1).nullable(),
  teams: z.array(z.object({ id: z.string().min(1), name: z.string() })),
  flowId: z.string().min(1).nullable(),
});
type PanelStatus = z.infer<typeof panelStatusSchema>;
type Action = "start" | "finish" | "disconnect";
const disconnected: PanelStatus = { connected: false, teamName: null, teamId: null, teams: [], flowId: null };
const commands = { start: "start_charm_hyper_panel_connection", finish: "finish_charm_hyper_panel_connection", disconnect: "disconnect_charm_hyper_panel" } as const;

function errorMessage(cause: unknown): string {
  if (!(cause instanceof z.ZodError)) {
    if (typeof cause === "object" && cause !== null && "message" in cause && typeof cause.message === "string" && cause.message) return cause.message;
    if (typeof cause === "string" && cause) return cause;
  }
  return "Não foi possível confirmar a conexão do painel Hyper.";
}

export function HyperPanelConnection({ alias, disabled, onBusyChange }: { alias: string; disabled: boolean; onBusyChange: (busy: boolean) => void }) {
  const bootstrap = useBootstrapResources();
  const teamLabel = useId();
  const callback = useRef(onBusyChange);
  useEffect(() => { callback.current = onBusyChange; }, [onBusyChange]);
  const version = useRef(0);
  const lock = useRef(false);
  const [status, setStatus] = useState<PanelStatus | null>(null);
  const [action, setAction] = useState<Action | "status" | null>("status");
  const [teamId, setTeamId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const busy = action !== null;

  useEffect(() => {
    const request = ++version.current;
    lock.current = true; callback.current(true);
    void invoke<unknown>("get_charm_hyper_panel_status", { alias }).then(result => {
      if (request === version.current) setStatus(panelStatusSchema.parse(result));
    }).catch((cause: unknown) => {
      if (request === version.current) setError(errorMessage(cause));
    }).finally(() => {
      if (request !== version.current) return;
      lock.current = false; setAction(null); callback.current(false);
    });
    return () => {
      version.current += 1;
      if (lock.current) { lock.current = false; callback.current(false); }
    };
  }, [alias]);

  async function perform(nextAction: Action, flow?: { flowId: string; teamId: string | null }) {
    if (lock.current || disabled) return;
    const request = ++version.current;
    lock.current = true; setAction(nextAction); setError(null); callback.current(true);
    try {
      const result = await invoke<unknown>(commands[nextAction], { alias, ...flow });
      if (request !== version.current) return;
      let next: PanelStatus;
      if (nextAction === "disconnect") {
        z.null().optional().parse(result);
        next = disconnected;
      } else {
        next = panelStatusSchema.parse(result);
        if (nextAction === "start" && !next.flowId) throw new Error("Não foi possível iniciar a conexão do painel Hyper. Tente novamente.");
      }
      setStatus(next); setTeamId(null);
      if (nextAction === "disconnect" || nextAction === "finish" && next.connected) {
        bootstrap?.updateUsage(alias, { data: null, error: false });
        // Usage is cached by alias and its existing hooks refresh on focus.
        window.dispatchEvent(new Event("focus"));
        toast.success(nextAction === "disconnect" ? "Cotas do painel Hyper desconectadas" : "Painel Hyper conectado");
      }
    } catch (cause: unknown) {
      if (request !== version.current) return;
      const message = errorMessage(cause);
      setError(message); toast.error(message);
    } finally {
      if (request === version.current) { lock.current = false; setAction(null); callback.current(false); }
    }
  }

  return <FieldSet className="rounded-lg border p-3" aria-busy={busy}>
    <FieldLegend variant="label">Cotas da assinatura Hyper</FieldLegend>
    <FieldDescription>Requer a extensão do Jarvis e a aba do painel aberta no navegador. A sessão permanece no navegador.</FieldDescription>
    {action === "status" ? <RowsSkeleton label="Consultando conexão do painel Hyper" count={1} /> : <FieldGroup className="gap-3">
      {status?.connected && <p role="status" className="text-sm">Conectado à equipe {status.teamName ?? status.teamId ?? "selecionada"}.</p>}
      {status?.flowId && <>
        <p role="status" className="text-sm">Conclua o login no Hyper no navegador e clique em Verificar conexão.</p>
        <FieldDescription>A conexão expira em 10 minutos. Você pode fechar esta tela enquanto conclui o login.</FieldDescription>
        {status.teams.length > 1 && <Field data-disabled={disabled || busy}>
          <FieldLabel htmlFor={teamLabel}>Equipe do Hyper</FieldLabel>
          <Select items={status.teams.map(team => ({ value: team.id, label: team.name }))} value={teamId} disabled={disabled || busy} onValueChange={value => setTeamId(value)}>
            <SelectTrigger id={teamLabel} aria-label="Equipe do Hyper" className="w-full cursor-pointer"><SelectValue placeholder="Selecione uma equipe" /></SelectTrigger>
            <SelectContent>{status.teams.map(team => <SelectItem key={team.id} value={team.id} className="cursor-pointer">{team.name}</SelectItem>)}</SelectContent>
          </Select>
        </Field>}
      </>}
      <div className="flex flex-wrap gap-2">
        <Button type="button" variant="outline" className="cursor-pointer" disabled={disabled || busy} onClick={() => void perform("start")}>{action === "start" ? "Abrindo painel…" : status?.connected ? "Reconectar painel" : "Conectar painel"}</Button>
        {status?.flowId && <Button type="button" className="cursor-pointer" disabled={disabled || busy || status.teams.length > 1 && !teamId} onClick={() => void perform("finish", { flowId: status.flowId!, teamId })}>{action === "finish" ? "Verificando…" : "Verificar conexão"}</Button>}
        {status?.connected && <Button type="button" variant="ghost" className="cursor-pointer" disabled={disabled || busy} onClick={() => void perform("disconnect")}>{action === "disconnect" ? "Desconectando…" : "Desconectar cotas"}</Button>}
      </div>
    </FieldGroup>}
    {error && <Alert variant="destructive"><AlertCircle /><AlertDescription>{error}</AlertDescription></Alert>}
    <FieldDescription>Desconectar cotas preserva a chave de API e o login no navegador.</FieldDescription>
  </FieldSet>;
}
