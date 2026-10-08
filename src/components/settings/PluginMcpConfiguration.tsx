import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import { toast } from "sonner";
import { pluginError } from "@/core/plugins";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/TextInput";
import { Spinner } from "@/components/ui/spinner";

const requirementsSchema = z.object({ fields: z.array(z.string()), configured: z.boolean() });
type Requirements = z.infer<typeof requirementsSchema>;

export function PluginMcpConfiguration({ serverId, name, active, disabled, onBusyChange }: { serverId: string; name: string; active: boolean; disabled: boolean; onBusyChange: (busy: boolean) => void }) {
  const [requirements, setRequirements] = useState<Requirements | null>(null);
  const [open, setOpen] = useState(false); const [values, setValues] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null); const [busy, setBusy] = useState(false);
  const mounted = useRef(false); const pending = useRef(false);
  useEffect(() => {
    let alive = true; mounted.current = true;
    if (active) void invoke("plugin_mcp_requirements", { id: serverId }).then(raw => { if (alive) setRequirements(requirementsSchema.parse(raw)); }).catch(() => { if (alive) setError("Não foi possível verificar a configuração deste MCP."); });
    return () => { alive = false; mounted.current = false; };
  }, [serverId, active]);
  useEffect(() => { onBusyChange(busy); return () => onBusyChange(false); }, [busy, onBusyChange]);
  function close() { if (!pending.current) { setOpen(false); setValues({}); setError(null); } }
  async function retryRequirements() {
    setError(null);
    try { const next = requirementsSchema.parse(await invoke("plugin_mcp_requirements", { id: serverId })); if (mounted.current) setRequirements(next); }
    catch { if (mounted.current) setError("Não foi possível verificar a configuração deste MCP."); }
  }
  async function save() {
    if (pending.current || !requirements) return;
    if (requirements.fields.some(field => !values[field]?.trim())) { setError("Preencha todos os campos solicitados."); return; }
    pending.current = true; setBusy(true); setError(null);
    try { const next = requirementsSchema.parse(await invoke("configure_plugin_mcp", { id: serverId, values })); if (mounted.current) { setRequirements(next); setOpen(false); setValues({}); toast.success("Credenciais atualizadas"); } }
    catch (cause) { if (mounted.current) setError(pluginError(cause)); }
    finally { pending.current = false; if (mounted.current) setBusy(false); }
  }
  if (!active) return <p className="mt-2 text-xs text-muted-foreground">Ative o plugin e este componente para verificar sua configuração.</p>;
  if (requirements?.fields.length === 0) return null;
  return <div className="mt-3 flex flex-col gap-2 border-t pt-3" aria-label={`Configuração de ${name}`}>
    <div className="flex flex-wrap items-center justify-between gap-2"><Badge variant="outline">{!requirements ? error ? "Verificação indisponível" : "Verificando configuração" : requirements.configured ? "Credenciais configuradas" : "Configuração necessária"}</Badge><Button variant="outline" size="sm" className="cursor-pointer" disabled={disabled || busy || (!requirements && !error)} onClick={() => { if (!requirements) { void retryRequirements(); return; } setValues({}); setError(null); setOpen(true); }}>{!requirements && error ? "Verificar novamente" : requirements?.configured ? "Alterar credenciais" : "Configurar credenciais"}</Button></div>
    {!open && error && <p role="alert" className="text-xs text-destructive">{error}</p>}
    <Dialog open={open} onOpenChange={value => { if (!value) close(); }}><DialogContent className="dark flex max-h-[85dvh] flex-col overflow-hidden sm:max-w-lg"><DialogHeader><DialogTitle>Configurar {name}</DialogTitle><DialogDescription>Informe os valores exigidos pelo servidor. Os valores salvos ficam protegidos e não são exibidos para os agentes.</DialogDescription></DialogHeader><form className="flex min-h-0 flex-col gap-4" onSubmit={event => { event.preventDefault(); void save(); }}><FieldGroup className="min-h-0 gap-4 overflow-y-auto px-1 pb-1">{requirements?.fields.map(field => <Field key={field}><FieldLabel htmlFor={`plugin-credential:${serverId}:${field}`} className="font-mono">{field}</FieldLabel><Input id={`plugin-credential:${serverId}:${field}`} type="password" autoComplete="new-password" value={values[field] ?? ""} disabled={busy} onChange={event => setValues(current => ({ ...current, [field]: event.target.value }))} /><FieldDescription>Para substituir a configuração, preencha todos os campos novamente.</FieldDescription></Field>)}{error && <p role="alert" className="text-sm text-destructive">{error}</p>}</FieldGroup><DialogFooter className="shrink-0"><Button type="button" variant="ghost" className="cursor-pointer" disabled={busy} onClick={close}>Cancelar</Button><Button type="submit" className="cursor-pointer" disabled={busy}>{busy && <Spinner data-icon="inline-start" />}Salvar credenciais</Button></DialogFooter></form></DialogContent></Dialog>
  </div>;
}
