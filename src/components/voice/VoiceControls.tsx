import { useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, type Ref } from "react";
import { listen } from "@tauri-apps/api/event";
import { Mic, MicOff, Phone, PhoneOff, Settings2, Square, Volume2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Hint } from "@/components/ui/hint";
import { useVoice } from "@/hooks/use-voice";
import { voiceActive, voicePhaseLabels, voiceQuestionAnswer, voiceQuestionPrompt, voiceReply, voiceTranscriptSchema } from "@/core/voice";
import type { ChatSnapshot } from "@/core/chat";
import type { PendingQuestion, QuestionAnswer, QuestionResponse } from "@/core/questions";
import { Robot } from "@/components/companion/Robot";
import { VoiceSettingsDialog } from "./VoiceSettings";
import { useRunningClock } from "@/hooks/use-running-clock";
import "./voice.css";

const errorMessage = (cause: unknown) => typeof cause === "string" ? cause : "Não foi possível usar a voz. Tente novamente.";
export interface VoiceControlHandle { startCall: () => Promise<void> }
export function VoiceControls({ target, onDictation, onMessage, snapshot, onAnswerQuestion, onPauseQuestion, proposal, onConfirmProject, disabled = false, callOnly = false, ref }: {
  target?: string; onDictation: (text: string) => void; onMessage: (text: string) => Promise<boolean>;
  snapshot?: ChatSnapshot | null; onAnswerQuestion?: (question: PendingQuestion, response: QuestionResponse) => Promise<boolean>;
  onPauseQuestion?: (question: PendingQuestion) => Promise<boolean>;
  disabled?: boolean; callOnly?: boolean;
  ref?: Ref<VoiceControlHandle>;
  proposal?: { id: string; projectName: string } | null;
  onConfirmProject?: (confirmed: boolean) => Promise<boolean>;
}) {
  const voice = useVoice();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const own = target && voice.session?.target === target;
  const active = Boolean(own && voice.active);
  const session = own ? voice.session : null;
  const latest = useRef({ voice, target, onDictation, onMessage, snapshot, onAnswerQuestion, onPauseQuestion, proposal, onConfirmProject });
  useLayoutEffect(() => { latest.current = { voice, target, onDictation, onMessage, snapshot, onAnswerQuestion, onPauseQuestion, proposal, onConfirmProject }; }, [voice, target, onDictation, onMessage, snapshot, onAnswerQuestion, onPauseQuestion, proposal, onConfirmProject]);
  const sequence = useRef(new Map<string, number>());
  const spoken = useRef(new Set<string>());
  const answers = useRef<{ toolId: string; values: QuestionAnswer[] } | null>(null);
  const ownedSession = useRef<string | null>(null);

  const control = async (action: "end" | "finish" | "interrupt" | "mute" | "unmute") => {
    if (!session?.id) return;
    try { await voice.control(session.id, action); setError(null); }
    catch (cause) { setError(errorMessage(cause)); }
  };
  const start = async (mode: "dictation" | "call") => {
    if (starting || disabled || !target) return;
    const installed = voice.settings?.models.find(model => model.id === voice.settings?.config.model)?.installed;
    if (!voice.settings?.config.enabled || !installed || mode === "call" && !voice.settings.speechReady) { setSettingsOpen(true); return; }
    setStarting(true); setError(null);
    spoken.current = new Set(snapshot?.turns.map(turn => `turn:${turn.id}`)); answers.current = null;
    try { const started = await voice.start(target, mode); ownedSession.current = started.id; }
    catch (cause) { setError(errorMessage(cause)); }
    finally { setStarting(false); }
  };
  const dictation = () => { if (active) void control(session?.phase === "preparing" ? "end" : session?.mode === "dictation" ? "finish" : "interrupt"); else void start("dictation"); };
  useImperativeHandle(ref, () => ({ startCall: () => start("call") }));

  useEffect(() => {
    let alive = true;
    const initial = latest.current.voice.session;
    if (latest.current.voice.active && initial && initial.target === target) ownedSession.current = initial.id;
    const subscription = listen("voice:transcript", event => {
      const result = voiceTranscriptSchema.safeParse(event.payload);
      if (!alive || !result.success) return;
      const value = result.data; const current = latest.current;
      if (value.target !== target || current.voice.session?.id !== value.sessionId || (sequence.current.get(value.sessionId) ?? 0) >= value.sequence) return;
      sequence.current.set(value.sessionId, value.sequence);
      if (value.mode === "dictation") { current.onDictation(value.text); return; }
      if (!voiceActive(current.voice.session)) return;
      void (async () => {
        try {
          if (current.proposal && current.onConfirmProject) {
            const answer = value.text.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLocaleLowerCase("pt-BR").replace(/[,.!?]/gu, "").trim();
            const confirmed = !answer.includes("nao") && /^(sim\b|confirmo\b|pode (iniciar|comecar|dar andamento)\b)/u.test(answer);
            const declined = /^(nao\b|agora nao\b|cancelar\b)/u.test(answer);
            if (confirmed || declined) {
              const accepted = await current.onConfirmProject(confirmed);
              if (!accepted || !confirmed) await current.voice.control(value.sessionId, "resume");
            } else await current.voice.control(value.sessionId, "speak", `Você quer continuar no projeto ${current.proposal.projectName}? Diga confirmo ou agora não.`);
            return;
          }
          const question = current.snapshot?.pendingQuestion;
          if (question && current.onAnswerQuestion) {
            if (answers.current?.toolId !== question.toolId) answers.current = { toolId: question.toolId, values: [] };
            const remaining = question.questions.find(item => !answers.current?.values.some(answer => answer.id === item.id));
            if (!remaining) return;
            answers.current.values.push(voiceQuestionAnswer(remaining, value.text));
            const next = question.questions.find(item => !answers.current?.values.some(answer => answer.id === item.id));
            if (next) { await current.voice.control(value.sessionId, "speak", voiceQuestionPrompt(next)); return; }
            const accepted = await current.onAnswerQuestion(question, { cancelled: false, answers: answers.current.values });
            if (!accepted) { answers.current = null; await current.voice.control(value.sessionId, "resume"); }
            return;
          }
          if (current.snapshot?.pendingApproval || current.snapshot?.pendingAuthoring) {
            current.onDictation(value.text);
            await current.voice.control(value.sessionId, "speak", "Essa ação precisa da sua aprovação nos controles da conversa. Mantive sua fala no campo de mensagem.");
            return;
          }
          const accepted = await current.onMessage(value.text);
          if (!accepted) { current.onDictation(value.text); await current.voice.control(value.sessionId, "resume"); }
        } catch (cause) { if (alive) setError(errorMessage(cause)); await current.voice.control(value.sessionId, "resume").catch(() => {}); }
      })();
    });
    void subscription.catch(cause => { if (alive) setError(errorMessage(cause)); });
    return () => {
      alive = false;
      void subscription.then(stop => stop()).catch(() => {});
      const id = ownedSession.current;
      if (id && latest.current.voice.session?.target === target) void latest.current.voice.control(id, "end").catch(() => {});
      ownedSession.current = null;
    };
  }, [target]);

  useEffect(() => {
    if (!active || session?.mode !== "call" || !snapshot || !session.id) return;
    const id = session.id;
    const question = snapshot.pendingQuestion;
    if (proposal && !spoken.current.has(`project:${proposal.id}`)) {
      spoken.current.add(`project:${proposal.id}`);
      snapshot.turns.forEach(turn => spoken.current.add(`turn:${turn.id}`));
      void voice.control(id, "speak", `Podemos continuar no projeto ${proposal.projectName}. Você confirma?`).catch(cause => setError(errorMessage(cause)));
      return;
    }
    const key = question ? `question:${question.toolId}` : snapshot.pendingApproval ? `approval:${snapshot.pendingApproval.tool.id}` : snapshot.pendingAuthoring ? `authoring:${snapshot.pendingAuthoring.toolId}` : null;
    const say = (text: string) => { void voice.control(id, "speak", text.slice(0, 12000)).catch(cause => setError(errorMessage(cause))); };
    if (key && !spoken.current.has(key)) {
      spoken.current.add(key);
      if (question) {
        answers.current = { toolId: question.toolId, values: [] };
        void onPauseQuestion?.(question).catch(() => {});
        say(voiceQuestionPrompt(question.questions[0]));
      } else say("Preciso de uma aprovação sua. Veja os controles da conversa para decidir.");
      return;
    }
    if (key || snapshot.activeTurnId || snapshot.queuedMessages?.length) return;
    const turn = [...snapshot.turns].reverse().find(turn => turn.status !== "running" && !spoken.current.has(`turn:${turn.id}`));
    if (turn) { snapshot.turns.forEach(item => { if (item.status !== "running") spoken.current.add(`turn:${item.id}`); }); say(voiceReply(turn)); }
  }, [active, session?.id, session?.mode, snapshot, voice, onPauseQuestion, proposal]);

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
      {!callOnly && <Hint content={active && session?.mode === "dictation" ? "Concluir ditado" : "Ditar mensagem · Ctrl/Cmd + Shift + Espaço"}><Button type="button" variant={active && session?.mode === "dictation" ? "secondary" : "ghost"} size="icon" className={`size-7.5 cursor-pointer rounded-full ${active && session?.mode === "dictation" ? "text-onedark-red" : "text-muted-foreground"}`} aria-label={active && session?.mode === "dictation" ? "Concluir ditado" : "Ditar mensagem"} aria-pressed={active && session?.mode === "dictation"} disabled={disabled || starting || !!voice.active && !own} onClick={dictation}>{active && session?.mode === "dictation" ? <Square className="size-3.5" /> : <Mic className="size-3.5" />}</Button></Hint>}
      <Hint content={active && session?.mode === "call" ? "Encerrar ligação" : "Ligar para o Jarvis"}><Button type="button" variant={active && session?.mode === "call" ? "destructive" : "ghost"} size="icon" className="size-7.5 cursor-pointer rounded-full" aria-label={active && session?.mode === "call" ? "Encerrar ligação" : "Ligar para o Jarvis"} disabled={disabled || starting || !!voice.active && (!own || session?.mode !== "call")} onClick={() => { if (active) void control("end"); else void start("call"); }}>{active && session?.mode === "call" ? <PhoneOff className="size-3.5" /> : <Phone className="size-3.5" />}</Button></Hint>
      {(active || error) && <Hint content="Configurações de voz"><Button type="button" size="icon" variant="ghost" className="size-7 cursor-pointer" aria-label="Configurações de voz" onClick={() => setSettingsOpen(true)}><Settings2 className="size-3" /></Button></Hint>}
    </div>
    <VoiceSettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
  </>;
}

export function VoiceSessionPanel({ target, compact = false, large = false }: { target?: string; compact?: boolean; large?: boolean }) {
  const voice = useVoice();
  const now = useRunningClock(voice.active);
  const session = target && voice.session?.target === target ? voice.session : null;
  const control = (action: "end" | "interrupt" | "mute" | "unmute") => {
    if (session?.id) void voice.control(session.id, action).catch(cause => toast.error(errorMessage(cause)));
  };
  if (!session || !voice.active) return session?.phase === "error" ? <p role="alert" className="text-xs text-destructive">{session.error}</p> : null;
  return <Card className={`voice-session-card ${compact ? "voice-compact" : ""} ${large ? "voice-call-stage" : ""}`} data-phase={session.phase}>
      <CardContent className="flex min-w-0 items-center gap-3 p-3">
        {session.mode === "call" && <div className="voice-avatar shrink-0"><Robot status={session.phase === "thinking" || session.phase === "transcribing" ? "running" : "idle"} gesture={session.phase === "listening" ? "listen" : session.phase === "speaking" ? "speak" : "none"} voiceLevel={session.level} expanded /></div>}
        <div className="min-w-0 flex-1">{session.mode === "call" && <p className="mb-1 font-mono text-[9px] text-muted-foreground">LIGAÇÃO · {Math.floor(Math.max(0, now - (session.startedAt ?? now)) / 60000).toString().padStart(2, "0")}:{Math.floor(Math.max(0, now - (session.startedAt ?? now)) / 1000 % 60).toString().padStart(2, "0")}</p>}<p role="status" className="flex items-center gap-2 text-xs font-medium text-primary"><span className="voice-meter" aria-hidden="true">{[0.4, 0.8, 1, 0.6, 0.9].map((scale, index) => <span key={index} style={{ transform: `scaleY(${Math.max(0.15, session.level * scale)})` }} />)}</span>{voicePhaseLabels[session.phase]}</p>{session.transcript && <p className="mt-1 line-clamp-2 break-words text-[11px] leading-4 text-muted-foreground">{session.speaker === "user" ? "Você" : "Jarvis"}: {session.transcript}</p>}{session.mode === "call" && <p className="mt-1 text-[10px] text-muted-foreground">{session.phase === "speaking" || session.phase === "synthesizing" ? "Toque no microfone para interromper e falar." : "Fale naturalmente; uma pausa envia sua mensagem."}</p>}</div>
        <div className="flex shrink-0 flex-col gap-1"><Hint content={session.phase === "speaking" || session.phase === "synthesizing" || session.phase === "thinking" ? "Interromper fala e ouvir" : session.muted ? "Ativar microfone" : "Pausar microfone"}><Button type="button" size="icon-sm" variant="secondary" className="cursor-pointer rounded-full" disabled={session.phase === "closing" || session.phase === "preparing"} aria-label={session.phase === "speaking" || session.phase === "synthesizing" || session.phase === "thinking" ? "Interromper fala e ouvir" : session.muted ? "Ativar microfone" : "Pausar microfone"} onClick={() => { void control(session.phase === "speaking" || session.phase === "synthesizing" || session.phase === "thinking" ? "interrupt" : session.muted ? "unmute" : "mute"); }}>{session.muted ? <MicOff className="size-3.5" /> : session.phase === "speaking" ? <Volume2 className="size-3.5" /> : <Mic className="size-3.5" />}</Button></Hint><Hint content="Encerrar voz"><Button type="button" size="icon-sm" variant={large ? "destructive" : "ghost"} className={`cursor-pointer rounded-full ${large ? "" : "text-destructive"}`} aria-label="Encerrar voz" onClick={() => { void control("end"); }}><PhoneOff className="size-3.5" /></Button></Hint></div>
      </CardContent>
    </Card>;
}
