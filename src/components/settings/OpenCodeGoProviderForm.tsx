import { useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AlertCircle, ExternalLink, KeyRound } from "lucide-react";
import { toast } from "sonner";
import { Input, InputGroupInput } from "@/components/TextInput";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field";
import { InputGroup, InputGroupAddon, InputGroupText } from "@/components/ui/input-group";
import { accountList, validateProviderDisplayName, type ProviderAccount } from "@/core/provider-accounts";
import { ProviderDisplayNameField } from "./ProviderDisplayNameField";
import { HyperPanelConnection } from "./HyperPanelConnection";

export function ApiKeyProviderForm({ account, providerKind = account?.providerKind === "brave-search" ? "brave-search" : account?.providerKind === "charm-hyper" ? "charm-hyper" : "opencode-go", onSaved, onCancel, onBusyChange }: {
  account?: ProviderAccount;
  providerKind?: "opencode-go" | "charm-hyper" | "brave-search";
  onSaved: (account: ProviderAccount) => void;
  onCancel: () => void;
  onBusyChange?: (busy: boolean) => void;
}) {
  const hyper = providerKind === "charm-hyper";
  const brave = providerKind === "brave-search";
  const providerName = brave ? "Brave Search" : hyper ? "Charm Hyper" : "OpenCode Go";
  const prefix = `${providerKind}-`;
  const [suffix, setSuffix] = useState(account?.alias.slice(prefix.length) ?? "");
  const [apiKey, setApiKey] = useState("");
  const [displayName, setDisplayName] = useState<string | null>(account?.displayName ?? null);
  const [saving, setSaving] = useState(false);
  const [panelBusy, setPanelBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [invalidField, setInvalidField] = useState<"alias" | "key" | null>(null);
  const lock = useRef(false);
  const panelLock = useRef(false);
  const alias = account?.alias ?? `${prefix}${suffix}`;
  const busy = saving || panelBusy;

  function panelBusyChanged(pending: boolean) {
    panelLock.current = pending; setPanelBusy(pending); onBusyChange?.(pending || lock.current);
  }

  async function save(event: FormEvent) {
    event.preventDefault();
    if (lock.current || panelLock.current) return;
    if (!suffix || suffix.length > 32 || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(suffix)) {
      setInvalidField("alias");
      setError("Use um sufixo de 1–32 caracteres: letras minúsculas, números e hífens internos.");
      return;
    }
    const key = apiKey.trim();
    if (!account && !key) { setInvalidField("key"); setError(`Informe a chave de API do ${providerName}.`); return; }
    const name = displayName ?? alias;
    if (!account || displayName !== null) {
      const nameError = validateProviderDisplayName(name);
      if (nameError) { setError(nameError); return; }
    }
    lock.current = true; setSaving(true); setError(null); setInvalidField(null); onBusyChange?.(true);
    try {
      const result = await invoke<unknown>(brave ? "save_brave_search_provider" : hyper ? "save_charm_hyper_provider" : "save_opencode_go_provider", { alias, ...(!account || displayName !== null ? { displayName: name.trim() } : {}), apiKey: key || null, editing: Boolean(account) });
      const saved = accountList([result])[0];
      if (!saved || saved.providerKind !== providerKind || saved.alias !== alias) throw new Error(`Não foi possível confirmar o cadastro do ${providerName}.`);
      setApiKey("");
      toast.success(`${providerName} ${account ? "atualizado" : brave ? "configurado" : "conectado"}`);
      onSaved(saved);
    } catch (cause: unknown) {
      const message = typeof cause === "object" && cause !== null && "message" in cause && typeof cause.message === "string" ? cause.message : typeof cause === "string" ? cause : `Não foi possível conectar o ${providerName}. Tente novamente.`;
      setError(key ? message.split(key).join("[chave ocultada]") : message);
    } finally { lock.current = false; setSaving(false); onBusyChange?.(false); }
  }

  return <form onSubmit={event => void save(event)} className="flex min-w-0 flex-col gap-5">
    <FieldGroup>
      <Field data-invalid={invalidField === "alias"}>
        <FieldLabel htmlFor="go-provider-alias">Sufixo do alias</FieldLabel>
        <InputGroup>
          <InputGroupAddon><InputGroupText className="font-mono text-xs text-onedark-cyan">{prefix}</InputGroupText></InputGroupAddon>
          <InputGroupInput id="go-provider-alias" value={suffix} onChange={event => setSuffix(event.currentTarget.value)} placeholder="pessoal" disabled={busy || Boolean(account)} aria-invalid={invalidField === "alias"} maxLength={32} autoComplete="off" className="min-w-0 font-mono text-xs" />
        </InputGroup>
        <FieldDescription className="text-xs">Letras minúsculas, números e hífens internos.</FieldDescription>
      </Field>
      <ProviderDisplayNameField value={displayName ?? alias} onChange={setDisplayName} disabled={busy} required={!account} />
      <Field data-invalid={invalidField === "key"}>
        <FieldLabel htmlFor="go-provider-api-key">Chave de API</FieldLabel>
        <Input id="go-provider-api-key" type="password" value={apiKey} onChange={event => setApiKey(event.currentTarget.value)} placeholder={account ? "Manter a chave atual" : `Sua chave do ${providerName}`} disabled={busy} aria-invalid={invalidField === "key"} autoComplete="new-password" />
        <FieldDescription className="text-xs">{account ? "Deixe em branco para manter a chave atual." : brave ? "Use uma chave da API Web Search do Brave. A chave será validada na primeira pesquisa, sem consumir cota ao cadastrar." : hyper ? "Use uma chave do painel Charm Hyper." : "Use a chave da sua assinatura. Os modelos e limites são consultados automaticamente."}</FieldDescription>
        {hyper && <FieldDescription className="text-xs">A chave de API consulta o catálogo e o saldo em Hypercredits. As cotas de 5 horas e semanais usam a sessão do painel.</FieldDescription>}
      </Field>
    </FieldGroup>
    {hyper && account?.providerKind === "charm-hyper" && <HyperPanelConnection key={account.alias} alias={account.alias} disabled={saving} onBusyChange={panelBusyChanged} />}
    <Button type="button" variant="link" disabled={busy} className="h-auto cursor-pointer self-start p-0 text-xs" onClick={() => void openUrl(brave ? "https://api-dashboard.search.brave.com/" : hyper ? "https://hyper.charm.land/" : "https://opencode.ai/go").catch(() => toast.error(`Não foi possível abrir o ${providerName}.`))}><ExternalLink data-icon="inline-start" />Abrir {providerName}</Button>
    {error && <Alert variant="destructive"><AlertCircle /><AlertDescription>{error}</AlertDescription></Alert>}
    <div className="flex flex-wrap justify-end gap-2">
      <Button type="button" variant="outline" disabled={busy} className="cursor-pointer" onClick={onCancel}>Cancelar</Button>
      <Button type="submit" disabled={busy} className="cursor-pointer"><KeyRound data-icon="inline-start" />{saving ? "Conectando…" : account ? "Salvar alterações" : `Conectar ${providerName}`}</Button>
    </div>
  </form>;
}

export { ApiKeyProviderForm as OpenCodeGoProviderForm };
