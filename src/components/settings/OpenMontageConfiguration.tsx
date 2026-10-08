import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, Download, KeyRound, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/TextInput";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { coreError } from "@/core/core-components";
import { openMontageConfigurationSchema, type OpenMontageConfiguration as Configuration } from "@/core/openmontage";

type ConfigurationProps = { open: boolean; onOpenChange: (open: boolean) => void; onSaved: () => Promise<void> };

export function OpenMontageConfiguration({ open, ...props }: ConfigurationProps) {
  return open ? <ConfigurationContent {...props} /> : null;
}

function ConfigurationContent({ onOpenChange, onSaved }: Omit<ConfigurationProps, "open">) {
  const [configuration, setConfiguration] = useState<Configuration | null>(null);
  const [credentials, setCredentials] = useState<Record<string, string>>({});
  const [removeCredentials, setRemoveCredentials] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [operation, setOperation] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const inFlight = useRef(false);
  useEffect(() => {
    let active = true;
    void invoke("get_openmontage_configuration").then(value => {
      const parsed = openMontageConfigurationSchema.parse(value);
      if (active) setConfiguration(parsed);
    }).catch(cause => { if (active) setError(coreError(cause)); });
    return () => { active = false; };
  }, [attempt]);

  async function save() {
    if (!configuration || inFlight.current) return;
    inFlight.current = true; setOperation("save"); setError(null);
    try {
      const secrets = Object.fromEntries(Object.entries(credentials).filter(([key, value]) => value.trim() && !removeCredentials.includes(key)).map(([key, value]) => [key, value.trim()]));
      const value = await invoke("save_openmontage_configuration", { configuration: { allowPaidTools: configuration.allowPaidTools, allowModelDownloads: configuration.allowModelDownloads, credentials: secrets, removeCredentials } });
      setConfiguration(openMontageConfigurationSchema.parse(value));
      setCredentials({}); setRemoveCredentials([]);
      await onSaved(); onOpenChange(false); toast.success("OpenMontage configurado");
    } catch (cause) { setError(coreError(cause)); }
    finally { inFlight.current = false; setOperation(null); }
  }

  async function install(id: string) {
    if (inFlight.current) return;
    inFlight.current = true; setOperation(id); setError(null);
    try {
      const value = await invoke("install_openmontage_optional_package", { id });
      const parsed = openMontageConfigurationSchema.parse(value);
      setConfiguration(current => current ? { ...parsed, allowPaidTools: current.allowPaidTools, allowModelDownloads: current.allowModelDownloads } : parsed);
      await onSaved(); toast.success("Recurso do OpenMontage preparado");
    } catch (cause) { setError(coreError(cause)); }
    finally { inFlight.current = false; setOperation(null); }
  }

  const busy = operation !== null;
  return <Dialog open onOpenChange={next => { if (!inFlight.current) onOpenChange(next); }}><DialogContent className="dark instrument-panel flex max-h-[85dvh] flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl" showCloseButton={!busy}>
    <DialogHeader className="shrink-0 border-b border-border px-6 py-5"><DialogTitle className="flex items-center gap-2"><KeyRound className="size-4 text-primary" />Configurar OpenMontage</DialogTitle><DialogDescription>Prepare as integrações e os recursos opcionais da produção de vídeos.</DialogDescription></DialogHeader>
    <div className="min-h-0 space-y-6 overflow-y-auto p-6">
      {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
      {!configuration && !error && <div role="status" aria-label="Carregando configuração do OpenMontage" className="space-y-3"><Skeleton className="h-24" /><Skeleton className="h-48" /><Skeleton className="h-32" /></div>}
      {!configuration && error && <Button variant="outline" onClick={() => { setError(null); setAttempt(value => value + 1); }}>Tentar novamente</Button>}
      {configuration && <>
        <div className="space-y-4">
          <div className="flex items-start justify-between gap-4"><div className="space-y-1"><Label htmlFor="openmontage-paid">Permitir ferramentas de API com cobrança</Label><p className="text-xs leading-5 text-muted-foreground">As execuções podem consumir créditos dos provedores. As assinaturas usadas para conversar não substituem as chaves destas APIs.</p></div><Switch id="openmontage-paid" checked={configuration.allowPaidTools} disabled={busy} className="cursor-pointer" onCheckedChange={allowPaidTools => setConfiguration({ ...configuration, allowPaidTools })} /></div>
          <div className="flex items-start justify-between gap-4"><div className="space-y-1"><Label htmlFor="openmontage-models">Permitir downloads de modelos</Label><p className="text-xs leading-5 text-muted-foreground">Pesos opcionais podem exigir vários GB e GPU. Os requisitos e as licenças dependem da ferramenta e do modelo.</p></div><Switch id="openmontage-models" checked={configuration.allowModelDownloads} disabled={busy} className="cursor-pointer" onCheckedChange={allowModelDownloads => setConfiguration({ ...configuration, allowModelDownloads })} /></div>
        </div>
        <section aria-label="Credenciais do OpenMontage" className="space-y-3 border-t border-border pt-5">
          <div><h2 className="text-sm font-medium">Configuração das ferramentas</h2><p className="mt-1 text-xs leading-5 text-muted-foreground">Chaves, endpoints e opções guardados no cofre do sistema. Configure apenas os serviços que deseja usar; os valores salvos não são exibidos.</p></div>
          <div className="grid gap-3 sm:grid-cols-2">{configuration.credentials.map(credential => {
            const removed = removeCredentials.includes(credential.key);
            return <div key={credential.key} className="space-y-2 rounded-md border border-border bg-card p-3">
              <div className="flex flex-wrap items-center justify-between gap-2"><Label htmlFor={`openmontage-${credential.key}`}>{credential.label}</Label><Badge variant="outline" className={`text-[10px] ${credential.configured && !removed ? "text-onedark-green" : "text-muted-foreground"}`}>{removed ? "Será removida" : credential.configured ? "Configurada" : "Não configurada"}</Badge></div>
              <Input id={`openmontage-${credential.key}`} type={credential.secret ? "password" : "text"} autoComplete="off" disabled={busy || removed} value={credentials[credential.key] ?? ""} onChange={event => setCredentials({ ...credentials, [credential.key]: event.target.value })} placeholder={credential.configured ? "Digite para substituir o valor" : credential.secret ? "Chave ou token" : "Valor da configuração"} />
              {credential.configured && <Button size="sm" variant="ghost" disabled={busy} aria-label={`${removed ? "Manter" : "Remover"} ${credential.secret ? "chave" : "configuração"} de ${credential.label}`} className="h-7 px-1 text-xs" onClick={() => setRemoveCredentials(current => removed ? current.filter(key => key !== credential.key) : [...current, credential.key])}><Trash2 className="size-3" />{removed ? "Manter" : "Remover"}</Button>}
            </div>;
          })}</div>
        </section>
        <section aria-label="Recursos locais do OpenMontage" className="space-y-3 border-t border-border pt-5"><div><h2 className="text-sm font-medium">Recursos locais opcionais</h2><p className="mt-1 text-xs leading-5 text-muted-foreground">Instalar as dependências habilita a preparação das ferramentas. Modelos, hardware e credenciais ainda podem ser necessários.</p></div>{configuration.optionalPackages.map(pkg => <div key={pkg.id} className="flex items-center justify-between gap-3 rounded-md border border-border bg-card p-3"><span className="text-sm">{pkg.label}</span>{pkg.installed ? <Badge variant="outline" className="gap-1 text-[10px] text-onedark-green"><Check className="size-3" />Dependências prontas</Badge> : <Button size="sm" variant="outline" disabled={busy} aria-label={`Instalar ${pkg.label}`} onClick={() => void install(pkg.id)}><Download className="size-3" />{operation === pkg.id ? "Preparando…" : "Instalar"}</Button>}</div>)}</section>
      </>}
    </div>
    <DialogFooter className="shrink-0 border-t border-border px-6 py-4"><Button variant="outline" disabled={busy} onClick={() => onOpenChange(false)}>Fechar</Button><Button disabled={!configuration || busy} onClick={() => void save()}>{operation === "save" ? "Salvando…" : "Salvar configuração"}</Button></DialogFooter>
  </DialogContent></Dialog>;
}
