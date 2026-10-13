import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Bell, Bot, Languages, MessageSquareText, Monitor, Moon, Send, Sparkles, TimerReset } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/TextInput";
import { ModelPicker } from "@/components/chat/ModelPicker";
import { accountGroups } from "./workflow/workflow-models";
import { executionChoice, executionSelection } from "@/core/executors";
import type { ProviderAccount } from "@/core/provider-accounts";
import { libraryError } from "@/core/library";
import { responseLanguages, sleepModes, systemSnapshotSchema, type SystemPreferences, type SystemSnapshot } from "@/core/system-preferences";

const systemError = (cause: unknown, fallback: string) => typeof cause === "string" ? cause : libraryError(cause, fallback);

export function SystemSettings({ accounts = [] }: { accounts?: ProviderAccount[] }) {
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [timeoutDraft, setTimeoutDraft] = useState("30");
  const lock = useRef(false);
  const applySnapshot = useCallback((next: SystemSnapshot) => {
    setSnapshot(next);
    setTimeoutDraft(String(next.preferences.askUserTimeoutSeconds));
  }, []);
  useEffect(() => {
    let alive = true; let changed = false; let unlisten: (() => void) | undefined;
    const start = async () => {
      try {
        const stop = await listen("system:changed", event => {
          const data = systemSnapshotSchema.safeParse(event.payload);
          if (alive && data.success) { changed = true; applySnapshot(data.data); }
        });
        if (!alive) { stop(); return; } unlisten = stop;
        const initial = systemSnapshotSchema.parse(await invoke("get_system_preferences"));
        if (alive && !changed) applySnapshot(initial);
      } catch (cause) { if (alive) setError(systemError(cause, "Não foi possível carregar as preferências.")); }
    };
    void start();
    return () => { alive = false; unlisten?.(); };
  }, [applySnapshot, attempt]);

  const save = async (patch: Partial<SystemPreferences>) => {
    if (!snapshot || lock.current) return;
    lock.current = true; setBusy(true); setError(null);
    try { applySnapshot(systemSnapshotSchema.parse(await invoke("save_system_preferences", { preferences: { ...snapshot.preferences, ...patch } }))); }
    catch (cause) { setError(systemError(cause, "Não foi possível salvar as preferências.")); }
    finally { lock.current = false; setBusy(false); }
  };
  const test = async () => {
    if (lock.current) return;
    lock.current = true; setBusy(true); setError(null);
    try { await invoke("test_system_notification"); toast.success("Notificação enviada ao sistema", { description: "Se o aviso não aparecer, confira as notificações do Jarvis e o modo Não incomodar nos ajustes do sistema." }); }
    catch (cause) { setError(systemError(cause, "Não foi possível enviar a notificação.")); }
    finally { lock.current = false; setBusy(false); }
  };
  const commitQuestionTimeout = () => {
    if (!snapshot) return;
    const value = Number(timeoutDraft);
    if (!Number.isInteger(value) || value < 1 || value > 3600) {
      setError("O tempo das perguntas deve ficar entre 1 e 3.600 segundos.");
      setTimeoutDraft(String(snapshot.preferences.askUserTimeoutSeconds));
      return;
    }
    if (value !== snapshot.preferences.askUserTimeoutSeconds) void save({ askUserTimeoutSeconds: value });
  };
  const problem = error ?? snapshot?.sleepError ?? snapshot?.notificationError ?? snapshot?.companionError;
  return <section aria-labelledby="system-settings-title" className="space-y-3">
    <h2 id="system-settings-title" className="settings-section-title"><Monitor aria-hidden="true" className="size-4" />Sistema</h2>
    {!snapshot ? error ? <div className="space-y-2"><p role="alert" className="text-xs text-destructive">{error}</p><Button size="sm" variant="outline" className="cursor-pointer" onClick={() => { setError(null); setAttempt(n => n + 1); }}>Tentar novamente</Button></div> : <Card role="status" aria-label="Carregando preferências do sistema" size="sm" className="settings-group">{[0, 1, 2, 3, 4, 5, 6].map((key) => <div key={key} className="settings-row"><div className="settings-row-copy space-y-2"><Skeleton className="h-4 w-36" /><Skeleton className="h-3 w-3/4" /></div><Skeleton className="h-8 w-56 max-w-full" /></div>)}</Card> : <>
      <Card role="region" aria-label="Preferências do sistema" size="sm" className="settings-group">
        <div className="settings-row">
          <div className="settings-row-copy">
            <Label htmlFor="response-language" className="flex cursor-pointer items-center gap-2 text-sm font-medium"><Languages aria-hidden="true" className="size-4 text-muted-foreground" />Idioma dos agentes</Label>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">Define o idioma das respostas. A interface permanece em pt-BR.</p>
          </div>
          <div className="settings-row-control w-full sm:w-64">
            <Select value={snapshot.preferences.responseLanguage} disabled={busy} onValueChange={value => { if (value && value in responseLanguages) void save({ responseLanguage: value as SystemPreferences["responseLanguage"] }); }}>
              <SelectTrigger id="response-language" size="sm" className="w-full cursor-pointer text-sm"><SelectValue>{responseLanguages[snapshot.preferences.responseLanguage]}</SelectValue></SelectTrigger>
              <SelectContent>{Object.entries(responseLanguages).map(([value, label]) => <SelectItem key={value} value={value} className="cursor-pointer text-xs">{label}</SelectItem>)}</SelectContent>
            </Select>
          </div>
        </div>
        <div className="settings-row">
          <div className="settings-row-copy">
            <Label htmlFor="prevent-sleep" className="flex cursor-pointer items-center gap-2 text-sm font-medium"><Moon aria-hidden="true" className="size-4 text-muted-foreground" />Impedir repouso</Label>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">Evita o repouso durante tarefas ou enquanto o Jarvis estiver aberto.</p>
          </div>
          <div className="settings-row-control w-full sm:w-80">
            <Select value={snapshot.preferences.preventSleep} disabled={busy} onValueChange={value => { if (value && value in sleepModes) void save({ preventSleep: value as SystemPreferences["preventSleep"] }); }}>
              <SelectTrigger id="prevent-sleep" size="sm" className="w-full cursor-pointer text-sm"><SelectValue>{sleepModes[snapshot.preferences.preventSleep]}</SelectValue></SelectTrigger>
              <SelectContent>{Object.entries(sleepModes).map(([value, label]) => <SelectItem key={value} value={value} className="cursor-pointer text-xs">{label}</SelectItem>)}</SelectContent>
            </Select>
          </div>
        </div>
        <div className="settings-row">
          <div className="settings-row-copy">
            <Label htmlFor="system-notifications" className="flex cursor-pointer items-center gap-2 text-sm font-medium"><Bell aria-hidden="true" className="size-4 text-muted-foreground" />Notificações do sistema</Label>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">Avisos sobre conclusões, perguntas pendentes e erros.</p>
          </div>
          <div className="settings-row-control gap-2">
            <Switch id="system-notifications" size="sm" checked={snapshot.preferences.notifications} disabled={busy} onCheckedChange={notifications => void save({ notifications })} className="shrink-0 cursor-pointer" />
            <Button size="sm" variant="ghost" disabled={busy || !snapshot.preferences.notifications} onClick={() => void test()} className="cursor-pointer gap-1.5 text-xs"><Send className="size-3" />Testar</Button>
          </div>
        </div>
        <div className="settings-row">
          <div className="settings-row-copy">
            <Label htmlFor="ask-user-timeout" className="flex cursor-pointer items-center gap-2 text-sm font-medium"><TimerReset aria-hidden="true" className="size-4 text-muted-foreground" />Resposta automática</Label>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">Aplica a opção recomendada quando uma pergunta ficar sem resposta.</p>
          </div>
          <div className="settings-row-control relative w-36">
            <Input id="ask-user-timeout" aria-label="Tempo para resposta recomendada" type="number" min={1} max={3600} step={1} inputMode="numeric" value={timeoutDraft} disabled={busy} onChange={event => setTimeoutDraft(event.target.value)} onBlur={commitQuestionTimeout} onKeyDown={event => { if (event.key === "Enter") event.currentTarget.blur(); }} className="h-7 pr-16 font-mono text-xs tabular-nums" />
            <span aria-hidden="true" className="pointer-events-none absolute inset-y-0 right-3 flex items-center text-[10px] text-muted-foreground">segundos</span>
          </div>
        </div>
        {snapshot.companionSupported && <div className="settings-row">
          <div className="settings-row-copy">
            <Label htmlFor="desktop-companion" className="flex cursor-pointer items-center gap-2 text-sm font-medium"><Bot aria-hidden="true" className="size-4 text-muted-foreground" />Assistente na área de trabalho</Label>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">Converse com o Jarvito, acompanhe atividades, responda perguntas e veja os limites de uso, mesmo com o Jarvis minimizado. Clique para abrir a ilha ou arraste a área preta do cabeçalho para reposicionar.</p>
          </div>
          <div className="settings-row-control"><Switch id="desktop-companion" size="sm" checked={snapshot.preferences.companionEnabled} disabled={busy} onCheckedChange={companionEnabled => void save({ companionEnabled })} className="shrink-0 cursor-pointer" /></div>
        </div>}
        <div className="settings-row">
          <div className="settings-row-copy">
            <p className="flex items-center gap-2 text-sm font-medium"><MessageSquareText aria-hidden="true" className="size-4 text-muted-foreground" />Títulos das conversas</p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">Escolha um modelo de um provedor conectado só para gerar títulos, sem alterar o modelo dos agentes.</p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">Automático segue o modelo da conversa; com Claude Code, usa um título local sem abrir outra sessão.</p>
          </div>
          <div className="settings-row-control min-w-0 flex-wrap gap-2">
            <ModelPicker modelGroups={accountGroups(accounts)} selection={executionSelection(snapshot.preferences.chatTitleModel)} disabled={busy} ariaLabel="Modelo para títulos das conversas" showProviderIdentity clearLabel="Automático" onClear={() => void save({ chatTitleModel: null })} onSelect={selection => void save({ chatTitleModel: executionChoice(selection) })} />
          </div>
        </div>
        <div className="settings-row">
          <div className="settings-row-copy">
            <p className="flex items-center gap-2 text-sm font-medium"><Sparkles aria-hidden="true" className="size-4 text-muted-foreground" />Refinar prompt</p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">Escolha o modelo e o nível de raciocínio usados para refinar seus rascunhos com CO-STAR, sem alterar o modelo do chat.</p>
          </div>
          <div className="settings-row-control min-w-0 flex-wrap gap-2">
            <ModelPicker modelGroups={accountGroups(accounts)} selection={executionSelection(snapshot.preferences.promptRefinementModel)} disabled={busy} ariaLabel="Modelo para refinar prompts" showProviderIdentity clearLabel="Não configurado" onClear={() => void save({ promptRefinementModel: null })} onSelect={selection => void save({ promptRefinementModel: executionChoice(selection) })} />
          </div>
        </div>
      </Card>
      {problem && <p role="alert" className="text-xs text-destructive">{problem}</p>}
    </>}
  </section>;
}
