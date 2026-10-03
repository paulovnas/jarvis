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
import { accountList, type ProviderAccount } from "@/core/provider-accounts";

const prefix = "opencode-go-";

export function OpenCodeGoProviderForm({ account, onSaved, onCancel, onBusyChange }: {
  account?: ProviderAccount;
  onSaved: (account: ProviderAccount) => void;
  onCancel: () => void;
  onBusyChange?: (busy: boolean) => void;
}) {
  const [suffix, setSuffix] = useState(account?.alias.slice(prefix.length) ?? "");
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [invalidField, setInvalidField] = useState<"alias" | "key" | null>(null);
  const lock = useRef(false);
  const alias = account?.alias ?? `${prefix}${suffix}`;

  async function save(event: FormEvent) {
    event.preventDefault();
    if (lock.current) return;
    if (!suffix || suffix.length > 32 || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(suffix)) {
      setInvalidField("alias");
      setError("Use um sufixo de 1–32 caracteres: letras minúsculas, números e hífens internos.");
      return;
    }
    const key = apiKey.trim();
    if (!account && !key) { setInvalidField("key"); setError("Informe a chave de API da sua assinatura OpenCode Go."); return; }
    lock.current = true; setSaving(true); setError(null); setInvalidField(null); onBusyChange?.(true);
    try {
      const result = await invoke<unknown>("save_opencode_go_provider", { alias, apiKey: key || null, editing: Boolean(account) });
      const saved = accountList([result])[0];
      if (!saved || saved.providerKind !== "opencode-go" || saved.alias !== alias) throw new Error("Não foi possível confirmar o cadastro do OpenCode Go.");
      setApiKey("");
      toast.success(account ? "OpenCode Go atualizado" : "OpenCode Go conectado");
      onSaved(saved);
    } catch (cause: unknown) {
      const message = typeof cause === "object" && cause !== null && "message" in cause && typeof cause.message === "string" ? cause.message : typeof cause === "string" ? cause : "Não foi possível conectar o OpenCode Go. Tente novamente.";
      setError(key ? message.split(key).join("[chave ocultada]") : message);
    } finally { lock.current = false; setSaving(false); onBusyChange?.(false); }
  }

  return <form onSubmit={event => void save(event)} className="flex min-w-0 flex-col gap-5">
    <FieldGroup>
      <Field data-invalid={invalidField === "alias"}>
        <FieldLabel htmlFor="go-provider-alias">Sufixo do alias</FieldLabel>
        <InputGroup>
          <InputGroupAddon><InputGroupText className="font-mono text-xs text-onedark-cyan">{prefix}</InputGroupText></InputGroupAddon>
          <InputGroupInput id="go-provider-alias" value={suffix} onChange={event => setSuffix(event.currentTarget.value)} placeholder="pessoal" disabled={saving || Boolean(account)} aria-invalid={invalidField === "alias"} maxLength={32} autoComplete="off" className="min-w-0 font-mono text-xs" />
        </InputGroup>
        <FieldDescription className="text-xs">Letras minúsculas, números e hífens internos.</FieldDescription>
      </Field>
      <Field data-invalid={invalidField === "key"}>
        <FieldLabel htmlFor="go-provider-api-key">Chave de API</FieldLabel>
        <Input id="go-provider-api-key" type="password" value={apiKey} onChange={event => setApiKey(event.currentTarget.value)} placeholder={account ? "Manter a chave atual" : "Sua chave do OpenCode Go"} disabled={saving} aria-invalid={invalidField === "key"} autoComplete="new-password" />
        <FieldDescription className="text-xs">{account ? "Deixe em branco para manter a chave atual." : "Use a chave da sua assinatura. Os modelos e limites são consultados automaticamente."}</FieldDescription>
      </Field>
    </FieldGroup>
    <Button type="button" variant="link" className="h-auto cursor-pointer self-start p-0 text-xs" onClick={() => void openUrl("https://opencode.ai/go").catch(() => toast.error("Não foi possível abrir o OpenCode Go."))}><ExternalLink data-icon="inline-start" />Abrir OpenCode Go</Button>
    {error && <Alert variant="destructive"><AlertCircle /><AlertDescription>{error}</AlertDescription></Alert>}
    <div className="flex flex-wrap justify-end gap-2">
      <Button type="button" variant="outline" disabled={saving} className="cursor-pointer" onClick={onCancel}>Cancelar</Button>
      <Button type="submit" disabled={saving} className="cursor-pointer"><KeyRound data-icon="inline-start" />{saving ? "Conectando…" : account ? "Salvar alterações" : "Conectar OpenCode Go"}</Button>
    </div>
  </form>;
}
