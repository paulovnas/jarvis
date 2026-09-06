import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Bell, Monitor, Moon, Send } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { libraryError } from "@/core/library";
import { sleepModes, systemSnapshotSchema, type SystemPreferences, type SystemSnapshot } from "@/core/system-preferences";

const systemError = (cause: unknown, fallback: string) => typeof cause === "string" ? cause : libraryError(cause, fallback);

export function SystemSettings() {
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const lock = useRef(false);
  useEffect(() => {
    let alive = true; let changed = false; let unlisten: (() => void) | undefined;
    const start = async () => {
      try {
        const stop = await listen("system:changed", event => {
          const data = systemSnapshotSchema.safeParse(event.payload);
          if (alive && data.success) { changed = true; setSnapshot(data.data); }
        });
        if (!alive) { stop(); return; } unlisten = stop;
        const initial = systemSnapshotSchema.parse(await invoke("get_system_preferences"));
        if (alive && !changed) setSnapshot(initial);
      } catch (cause) { if (alive) setError(systemError(cause, "Não foi possível carregar as preferências.")); }
    };
    void start();
    return () => { alive = false; unlisten?.(); };
  }, [attempt]);

  const save = async (patch: Partial<SystemPreferences>) => {
    if (!snapshot || lock.current) return;
    lock.current = true; setBusy(true); setError(null);
    try { setSnapshot(systemSnapshotSchema.parse(await invoke("save_system_preferences", { preferences: { ...snapshot.preferences, ...patch } }))); }
    catch (cause) { setError(systemError(cause, "Não foi possível salvar as preferências.")); }
    finally { lock.current = false; setBusy(false); }
  };
  const test = async () => {
    if (lock.current) return;
    lock.current = true; setBusy(true); setError(null);
    try { await invoke("test_system_notification"); toast.success("Notificação de teste enviada"); }
    catch (cause) { setError(systemError(cause, "Não foi possível enviar a notificação.")); }
    finally { lock.current = false; setBusy(false); }
  };
  const problem = error ?? snapshot?.sleepError ?? snapshot?.notificationError;
  return <section aria-labelledby="system-settings-title" className="space-y-3">
    <h2 id="system-settings-title" className="micro-label flex items-center gap-2 text-muted-foreground"><Monitor className="size-3.5" />Sistema</h2>
    {!snapshot ? error ? <div className="space-y-2"><p role="alert" className="text-xs text-destructive">{error}</p><Button size="sm" variant="outline" className="cursor-pointer" onClick={() => { setError(null); setAttempt(n => n + 1); }}>Tentar novamente</Button></div> : <div role="status" aria-label="Carregando preferências do sistema" className="grid gap-3 sm:grid-cols-2">{[0, 1].map(key => <Card key={key} className="gap-3 p-4"><Skeleton className="h-4 w-32" /><Skeleton className="h-9 w-full" /></Card>)}</div> : <>
      <div className="grid gap-3 sm:grid-cols-2">
        <Card className="min-w-0 gap-4 p-4">
          <Label htmlFor="prevent-sleep" className="flex items-center gap-2 text-xs"><Moon className="size-4 text-onedark-yellow" />Impedir repouso</Label>
          <Select value={snapshot.preferences.preventSleep} disabled={busy} onValueChange={value => { if (value && value in sleepModes) void save({ preventSleep: value as SystemPreferences["preventSleep"] }); }}>
            <SelectTrigger id="prevent-sleep" className="w-full cursor-pointer text-xs"><SelectValue>{sleepModes[snapshot.preferences.preventSleep]}</SelectValue></SelectTrigger>
            <SelectContent>{Object.entries(sleepModes).map(([value, label]) => <SelectItem key={value} value={value} className="cursor-pointer text-xs">{label}</SelectItem>)}</SelectContent>
          </Select>
        </Card>
        <Card className="min-w-0 gap-4 p-4">
          <div className="flex items-center justify-between gap-3"><Label htmlFor="system-notifications" className="flex items-center gap-2 text-xs"><Bell className="size-4 text-primary" />Notificações do sistema</Label><Switch id="system-notifications" checked={snapshot.preferences.notifications} disabled={busy} onCheckedChange={notifications => void save({ notifications })} className="cursor-pointer" /></div>
          <div className="flex items-center justify-between gap-3"><span className="text-[11px] text-muted-foreground">Conclusões, perguntas e erros</span><Button size="sm" variant="outline" disabled={busy || !snapshot.preferences.notifications} onClick={() => void test()} className="cursor-pointer gap-1.5 text-xs"><Send className="size-3" />Testar</Button></div>
        </Card>
      </div>
      {problem && <p role="alert" className="text-xs text-destructive">{problem}</p>}
    </>}
  </section>;
}
