import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Bell, Languages, Monitor, Moon, Send, TimerReset } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/TextInput";
import { libraryError } from "@/core/library";
import { responseLanguages, sleepModes, systemSnapshotSchema, type SystemPreferences, type SystemSnapshot } from "@/core/system-preferences";

const systemError = (cause: unknown, fallback: string) => typeof cause === "string" ? cause : libraryError(cause, fallback);

export function SystemSettings() {
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
  const problem = error ?? snapshot?.sleepError ?? snapshot?.notificationError;
  return <section aria-labelledby="system-settings-title" className="space-y-3">
    <h2 id="system-settings-title" className="micro-label flex items-center gap-2 text-muted-foreground"><Monitor className="size-3.5" />Sistema</h2>
    {!snapshot ? error ? <div className="space-y-2"><p role="alert" className="text-xs text-destructive">{error}</p><Button size="sm" variant="outline" className="cursor-pointer" onClick={() => { setError(null); setAttempt(n => n + 1); }}>Tentar novamente</Button></div> : <Card role="status" aria-label="Carregando preferências do sistema" size="sm" className="grid gap-0 rounded-lg py-0 lg:grid-cols-2">{[0, 1, 2, 3].map((key) => <div key={key} className={`space-y-3 p-4 ${key > 0 ? "border-t border-border" : ""} ${key % 2 === 1 ? "lg:border-l" : ""} ${key === 1 ? "lg:border-t-0" : ""}`}><Skeleton className="h-4 w-36" /><Skeleton className="h-3 w-3/4" /><Skeleton className="h-7 w-48" /></div>)}</Card> : <>
      <Card role="region" aria-label="Preferências do sistema" size="sm" className="grid gap-0 rounded-lg py-0 lg:grid-cols-2">
        <div className="flex min-w-0 flex-col gap-3 p-4 xl:flex-row xl:items-center xl:justify-between">
          <div className="min-w-0">
            <Label htmlFor="response-language" className="flex cursor-pointer items-center gap-2 text-xs font-medium"><Languages className="size-3.5 text-onedark-purple" />Idioma dos agentes</Label>
            <p className="mt-1 text-[11px] leading-4 text-muted-foreground">Define o idioma das respostas. A interface permanece em pt-BR.</p>
          </div>
          <div className="w-full shrink-0 xl:w-auto">
            <Select value={snapshot.preferences.responseLanguage} disabled={busy} onValueChange={value => { if (value && value in responseLanguages) void save({ responseLanguage: value as SystemPreferences["responseLanguage"] }); }}>
              <SelectTrigger id="response-language" size="sm" className="w-full cursor-pointer text-xs xl:w-52"><SelectValue>{responseLanguages[snapshot.preferences.responseLanguage]}</SelectValue></SelectTrigger>
              <SelectContent>{Object.entries(responseLanguages).map(([value, label]) => <SelectItem key={value} value={value} className="cursor-pointer text-xs">{label}</SelectItem>)}</SelectContent>
            </Select>
          </div>
        </div>
        <div className="flex min-w-0 flex-col gap-3 border-t border-border p-4 lg:border-t-0 lg:border-l xl:flex-row xl:items-center xl:justify-between">
          <div className="min-w-0">
            <Label htmlFor="prevent-sleep" className="flex cursor-pointer items-center gap-2 text-xs font-medium"><Moon className="size-3.5 text-onedark-yellow" />Impedir repouso</Label>
            <p className="mt-1 text-[11px] leading-4 text-muted-foreground">Evita o repouso durante tarefas ou enquanto o Jarvis estiver aberto.</p>
          </div>
          <div className="w-full shrink-0 xl:w-auto">
            <Select value={snapshot.preferences.preventSleep} disabled={busy} onValueChange={value => { if (value && value in sleepModes) void save({ preventSleep: value as SystemPreferences["preventSleep"] }); }}>
              <SelectTrigger id="prevent-sleep" size="sm" className="w-full cursor-pointer text-xs xl:w-64"><SelectValue>{sleepModes[snapshot.preferences.preventSleep]}</SelectValue></SelectTrigger>
              <SelectContent>{Object.entries(sleepModes).map(([value, label]) => <SelectItem key={value} value={value} className="cursor-pointer text-xs">{label}</SelectItem>)}</SelectContent>
            </Select>
          </div>
        </div>
        <div className="flex min-w-0 flex-col gap-3 border-t border-border p-4 xl:flex-row xl:items-center xl:justify-between">
          <div className="min-w-0">
            <Label htmlFor="system-notifications" className="flex cursor-pointer items-center gap-2 text-xs font-medium"><Bell className="size-3.5 text-primary" />Notificações do sistema</Label>
            <p className="mt-1 text-[11px] leading-4 text-muted-foreground">Avisos sobre conclusões, perguntas pendentes e erros.</p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Switch id="system-notifications" size="sm" checked={snapshot.preferences.notifications} disabled={busy} onCheckedChange={notifications => void save({ notifications })} className="shrink-0 cursor-pointer" />
            <Button size="sm" variant="ghost" disabled={busy || !snapshot.preferences.notifications} onClick={() => void test()} className="cursor-pointer gap-1.5 text-xs"><Send className="size-3" />Testar</Button>
          </div>
        </div>
        <div className="flex min-w-0 flex-col gap-3 border-t border-border p-4 lg:border-l xl:flex-row xl:items-center xl:justify-between">
          <div className="min-w-0">
            <Label htmlFor="ask-user-timeout" className="flex cursor-pointer items-center gap-2 text-xs font-medium"><TimerReset className="size-3.5 text-onedark-green" />Resposta automática</Label>
            <p className="mt-1 text-[11px] leading-4 text-muted-foreground">Aplica a opção recomendada quando uma pergunta ficar sem resposta.</p>
          </div>
          <div className="relative w-full shrink-0 xl:w-36">
            <Input id="ask-user-timeout" aria-label="Tempo para resposta recomendada" type="number" min={1} max={3600} step={1} inputMode="numeric" value={timeoutDraft} disabled={busy} onChange={event => setTimeoutDraft(event.target.value)} onBlur={commitQuestionTimeout} onKeyDown={event => { if (event.key === "Enter") event.currentTarget.blur(); }} className="h-7 pr-16 font-mono text-xs tabular-nums" />
            <span aria-hidden="true" className="pointer-events-none absolute inset-y-0 right-3 flex items-center text-[10px] text-muted-foreground">segundos</span>
          </div>
        </div>
      </Card>
      {problem && <p role="alert" className="text-xs text-destructive">{problem}</p>}
    </>}
  </section>;
}
