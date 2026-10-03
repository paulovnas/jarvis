import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Mic, MicOff, Settings2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Hint } from "@/components/ui/hint";
import { acceptsDictationTranscript, useVoice } from "@/hooks/use-voice";
import { voicePhaseLabels, voiceTranscriptSchema } from "@/core/voice";
import { VoiceSettingsDialog } from "./VoiceSettings";
import "./voice.css";

const errorMessage = (cause: unknown) => typeof cause === "string" ? cause : "Não foi possível usar a voz. Tente novamente.";

/** Dictation only appends text; the composer owns message submission. */
export function VoiceControls({ target, onDictation, disabled = false }: {
  target?: string;
  onDictation: (text: string) => void;
  disabled?: boolean;
}) {
  const voice = useVoice();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const own = Boolean(target && voice.session?.target === target && voice.session.mode === "dictation");
  const active = own && voice.active;
  const session = own ? voice.session : null;
  const latest = useRef({ voice, target, onDictation });
  useLayoutEffect(() => { latest.current = { voice, target, onDictation }; }, [voice, target, onDictation]);
  const sequence = useRef(new Map<string, number>());
  const ownedSession = useRef<string | null>(null);
  const scope = useRef({ active: false });

  const control = async (action: "end" | "finish") => {
    if (!session?.id) return;
    try { await voice.control(session.id, action); setError(null); }
    catch (cause) { setError(errorMessage(cause)); }
  };
  const start = async () => {
    if (starting || disabled || !target || voice.active && !own && voice.session?.mode !== "announcement") return;
    const installed = voice.settings?.models.find(model => model.id === voice.settings?.config.model)?.installed;
    if (!voice.settings?.config.enabled || !installed) { setSettingsOpen(true); return; }
    const owner = scope.current;
    setStarting(true); setError(null);
    try {
      const started = await voice.start(target, "dictation");
      if (!owner.active || latest.current.target !== target) {
        if (started.id) await voice.control(started.id, "end");
        return;
      }
      ownedSession.current = started.id;
    } catch (cause) { if (owner.active) setError(errorMessage(cause)); }
    finally { if (scope.current.active) setStarting(false); }
  };
  const dictation = () => { if (active) void control(session?.phase === "preparing" ? "end" : "finish"); else void start(); };

  useEffect(() => {
    const owner = { active: true };
    scope.current = owner;
    const initial = latest.current.voice.session;
    if (latest.current.voice.active && initial?.mode === "dictation" && initial.target === target) ownedSession.current = initial.id;
    const subscription = listen("voice:transcript", event => {
      const result = voiceTranscriptSchema.safeParse(event.payload);
      if (!owner.active || !result.success) return;
      const value = result.data;
      const current = latest.current;
      if (value.target !== target || current.voice.session?.mode !== "dictation" || current.voice.session.id !== value.sessionId || (sequence.current.get(value.sessionId) ?? 0) >= value.sequence) return;
      sequence.current.set(value.sessionId, value.sequence);
      if (acceptsDictationTranscript(value.sessionId)) current.onDictation(value.text);
    });
    void subscription.catch(cause => { if (owner.active) setError(errorMessage(cause)); });
    return () => {
      owner.active = false;
      void subscription.then(stop => stop()).catch(() => {});
      const id = ownedSession.current;
      const current = latest.current.voice.session;
      if (id && current?.id === id && current.target === target && current.mode === "dictation") void latest.current.voice.control(id, "end").catch(() => {});
      ownedSession.current = null;
    };
  }, [target]);

  useEffect(() => { if (error) toast.error(error, { id: "voice-error" }); }, [error]);
  useEffect(() => {
    const shortcut = (event: KeyboardEvent) => {
      if (!event.repeat && (event.ctrlKey || event.metaKey) && event.shiftKey && event.code === "Space") { event.preventDefault(); dictation(); }
      if (event.key === "Escape" && active) { event.preventDefault(); void control("end"); }
    };
    document.addEventListener("keydown", shortcut);
    return () => document.removeEventListener("keydown", shortcut);
  });

  return <>
    <div className="inline-flex shrink-0 items-center gap-0.5" aria-label="Jarvis Voice">
      <Hint content={active ? "Concluir ditado e desligar microfone" : "Ditar mensagem · Ctrl/Cmd + Shift + Espaço"}><Button type="button" variant={active ? "secondary" : "ghost"} size="icon" className={`size-7.5 cursor-pointer rounded-full ${active ? "text-onedark-red" : "text-muted-foreground"}`} aria-label={active ? "Concluir ditado" : "Ditar mensagem"} aria-pressed={active} disabled={disabled && !active || starting || voice.active && !own && voice.session?.mode !== "announcement"} onClick={dictation}>{active ? <MicOff className="size-3.5" /> : <Mic className="size-3.5" />}</Button></Hint>
      {(active || error) && <Hint content="Configurações de voz"><Button type="button" size="icon" variant="ghost" className="size-7 cursor-pointer" aria-label="Configurações de voz" onClick={() => setSettingsOpen(true)}><Settings2 className="size-3" /></Button></Hint>}
    </div>
    <VoiceSettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
  </>;
}

export function VoiceSessionPanel({ target }: { target?: string }) {
  const voice = useVoice();
  const session = target && voice.session?.target === target && voice.session.mode === "dictation" ? voice.session : null;
  if (!session || !voice.active) return session?.phase === "error" ? <p role="alert" className="text-xs text-destructive">{session.error}</p> : null;
  return <div role="status" className="voice-dictation-status" data-phase={session.phase}>
    <span className="voice-meter" aria-hidden="true">{[0.4, 0.8, 1, 0.6, 0.9].map((scale, index) => <span key={index} style={{ transform: `scaleY(${Math.max(0.15, session.level * scale)})` }} />)}</span>
    <span>{voicePhaseLabels[session.phase]}</span><span className="voice-dictation-hint">Uma pausa insere sua fala no campo.</span>
  </div>;
}
