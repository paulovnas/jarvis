import { useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, type Ref } from "react";
import { listen } from "@tauri-apps/api/event";
import { Mic, MicOff, PhoneOff, Settings2, Volume2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Hint } from "@/components/ui/hint";
import { acceptsDictationTranscript, useVoice } from "@/hooks/use-voice";
import { voiceActive, voicePhaseLabels, voiceQuestionAnswer, voiceQuestionPrompt, voiceReply, voiceTranscriptSchema } from "@/core/voice";
import type { ChatSnapshot } from "@/core/chat";
import type { PendingQuestion, QuestionAnswer, QuestionResponse } from "@/core/questions";
import { Robot } from "@/components/companion/Robot";
import { VoiceSettingsDialog } from "./VoiceSettings";
import { useRunningClock } from "@/hooks/use-running-clock";
import "./voice.css";

const errorMessage = (cause: unknown) => typeof cause === "string" ? cause : "Não foi possível usar a voz. Tente novamente.";
const thinkingPhrases = ["Certo, deixa eu ver.", "Uhum, só um momento.", "Beleza, estou conferindo.", "Deixa eu olhar isso para você.", "Só mais um instante.", "Estou vendo os detalhes."];
export interface VoiceControlHandle { startCall: () => Promise<void> }
export function VoiceControls({ target, onDictation, onMessage, snapshot, onAnswerQuestion, onPauseQuestion, proposal, onConfirmProject, disabled = false, allowCall = false, speechEnabled = true, ref }: {
  target?: string; onDictation: (text: string) => void; onMessage: (text: string) => Promise<boolean>;
  snapshot?: ChatSnapshot | null; onAnswerQuestion?: (question: PendingQuestion, response: QuestionResponse) => Promise<boolean>;
  onPauseQuestion?: (question: PendingQuestion) => Promise<boolean>;
  disabled?: boolean; allowCall?: boolean; speechEnabled?: boolean;
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
  const latest = useRef({ voice, target, onDictation, onMessage, snapshot, onAnswerQuestion, onPauseQuestion, proposal, onConfirmProject, speechEnabled });
  useLayoutEffect(() => { latest.current = { voice, target, onDictation, onMessage, snapshot, onAnswerQuestion, onPauseQuestion, proposal, onConfirmProject, speechEnabled }; }, [voice, target, onDictation, onMessage, snapshot, onAnswerQuestion, onPauseQuestion, proposal, onConfirmProject, speechEnabled]);
  const sequence = useRef(new Map<string, number>());
  const spoken = useRef(new Set<string>());
  const answers = useRef<{ toolId: string; values: QuestionAnswer[] } | null>(null);
  const ownedSession = useRef<string | null>(null);
  const cue = useRef({ turnId: "", startedAt: 0, lastAt: 0, phrase: 0 });

  const control = async (action: "end" | "finish" | "interrupt" | "mute" | "unmute") => {
    if (!session?.id) return;
    try { await voice.control(session.id, action); setError(null); }
    catch (cause) { setError(errorMessage(cause)); }
  };
  const start = async (mode: "dictation" | "call") => {
    if (starting || disabled || !target || mode === "call" && !allowCall) return;
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
      if (value.mode === "dictation") { if (acceptsDictationTranscript(value.sessionId)) current.onDictation(value.text); return; }
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
            } else if (current.speechEnabled) await current.voice.control(value.sessionId, "speak", `Você quer continuar no projeto ${current.proposal.projectName}? Diga confirmo ou agora não.`);
            else await current.voice.control(value.sessionId, "resume");
            return;
          }
          const question = current.snapshot?.pendingQuestion;
          if (question && current.onAnswerQuestion) {
            if (answers.current?.toolId !== question.toolId) answers.current = { toolId: question.toolId, values: [] };
            const remaining = question.questions.find(item => !answers.current?.values.some(answer => answer.id === item.id));
            if (!remaining) return;
            answers.current.values.push(voiceQuestionAnswer(remaining, value.text));
            const next = question.questions.find(item => !answers.current?.values.some(answer => answer.id === item.id));
            if (next) { await current.voice.control(value.sessionId, current.speechEnabled ? "speak" : "resume", current.speechEnabled ? voiceQuestionPrompt(next) : undefined); return; }
            const accepted = await current.onAnswerQuestion(question, { cancelled: false, answers: answers.current.values });
            if (!accepted) { answers.current = null; await current.voice.control(value.sessionId, "resume"); }
            return;
          }
          if (current.snapshot?.pendingApproval || current.snapshot?.pendingAuthoring) {
            current.onDictation(value.text);
            await current.voice.control(value.sessionId, current.speechEnabled ? "speak" : "resume", current.speechEnabled ? "Essa ação precisa da sua aprovação nos controles da conversa. Mantive sua fala no campo de mensagem." : undefined);
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
      void voice.control(id, speechEnabled ? "speak" : "resume", speechEnabled ? `Podemos continuar no projeto ${proposal.projectName}. Você confirma?` : undefined).catch(cause => setError(errorMessage(cause)));
      return;
    }
    const key = question ? `question:${question.turnId}:${question.toolId}` : snapshot.pendingApproval ? `approval:${snapshot.pendingApproval.tool.id}` : snapshot.pendingAuthoring ? `authoring:${snapshot.pendingAuthoring.toolId}` : null;
    const say = (text: string) => { void voice.control(id, speechEnabled ? "speak" : "resume", speechEnabled ? text.slice(0, 12000) : undefined).catch(cause => setError(errorMessage(cause))); };
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
  }, [active, session?.id, session?.mode, snapshot, voice, onPauseQuestion, proposal, speechEnabled]);

  useEffect(() => {
    const turnId = snapshot?.activeTurnId;
    if (!active || session?.mode !== "call" || !turnId) { cue.current.turnId = ""; return; }
    if (cue.current.turnId !== turnId) cue.current = { ...cue.current, turnId, startedAt: Date.now(), lastAt: 0 };
    const timer = setInterval(() => {
      const current = latest.current, value = current.voice.session, chat = current.snapshot;
      if (!current.speechEnabled || !current.voice.active || !value?.id || value.mode !== "call" || value.target !== current.target || value.phase !== "thinking" || value.muted || chat?.turns.find(turn => turn.id === chat.activeTurnId)?.steps.some(step => step.retry) || chat?.pendingQuestion || chat?.pendingApproval || chat?.pendingAuthoring || current.proposal || chat?.activeTurnId !== cue.current.turnId) return;
      const now = Date.now();
      if (now - cue.current.startedAt < 4000 || cue.current.lastAt && now - cue.current.lastAt < 20000) return;
      cue.current.lastAt = now;
      const phrase = thinkingPhrases[cue.current.phrase++ % thinkingPhrases.length];
      void current.voice.control(value.id, "cue", phrase).catch(() => {});
    }, 1000);
    return () => clearInterval(timer);
  }, [active, session?.id, session?.mode, snapshot?.activeTurnId]);

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
    <div className={active && session?.mode === "call" ? "hidden" : "inline-flex shrink-0 items-center gap-0.5"} aria-label="Jarvis Voice" hidden={active && session?.mode === "call"}>
      <Hint content={active && session?.mode === "dictation" ? "Concluir ditado e desligar microfone" : "Ditar mensagem · Ctrl/Cmd + Shift + Espaço"}><Button type="button" variant={active && session?.mode === "dictation" ? "secondary" : "ghost"} size="icon" className={`size-7.5 cursor-pointer rounded-full ${active && session?.mode === "dictation" ? "text-onedark-red" : "text-muted-foreground"}`} aria-label={active && session?.mode === "dictation" ? "Concluir ditado" : "Ditar mensagem"} aria-pressed={active && session?.mode === "dictation"} disabled={disabled && !active || starting || !!voice.active && !own && voice.session?.mode !== "announcement"} onClick={dictation}>{active && session?.mode === "dictation" ? <MicOff className="size-3.5" /> : <Mic className="size-3.5" />}</Button></Hint>
      {(active || error) && <Hint content="Configurações de voz"><Button type="button" size="icon" variant="ghost" className="size-7 cursor-pointer" aria-label="Configurações de voz" onClick={() => setSettingsOpen(true)}><Settings2 className="size-3" /></Button></Hint>}
    </div>
    <VoiceSettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
  </>;
}

export function VoiceSessionPanel({ target, compact = false }: { target?: string; compact?: boolean }) {
  const voice = useVoice();
  const now = useRunningClock(voice.active);
  const session = target && voice.session?.target === target ? voice.session : null;
  const control = (action: "end" | "interrupt" | "mute" | "unmute") => {
    if (session?.id) void voice.control(session.id, action).catch(cause => toast.error(errorMessage(cause)));
  };
  if (!session || !voice.active) return session?.phase === "error" ? <p role="alert" className="text-xs text-destructive">{session.error}</p> : null;
  const meter = <span className="voice-meter" aria-hidden="true">{[0.4, 0.8, 1, 0.6, 0.9].map((scale, index) => <span key={index} style={{ transform: `scaleY(${Math.max(0.15, session.level * scale)})` }} />)}</span>;
  if (session.mode !== "call") return <div role="status" className="voice-dictation-status" data-phase={session.phase}>{meter}<span>{voicePhaseLabels[session.phase]}</span><span className="voice-dictation-hint">Uma pausa insere sua fala no campo.</span></div>;
  const interrupting = session.phase === "speaking" || session.phase === "synthesizing" || session.phase === "thinking";
  const microphoneLabel = interrupting ? "Interromper fala e ouvir" : session.muted ? "Ativar microfone" : "Pausar microfone";
  const elapsed = Math.max(0, now - (session.startedAt ?? now));
  return <Card className={`voice-call-stage min-h-0 gap-0 p-0 ${compact ? "voice-call-compact" : ""}`} data-phase={session.phase} aria-label="Ligação com Jarvito">
    <CardContent className="voice-call-content">
      <div className="voice-avatar"><Robot status={session.phase === "thinking" || session.phase === "transcribing" ? "running" : "idle"} gesture={session.phase === "listening" ? "listen" : session.phase === "speaking" ? "speak" : "none"} voiceLevel={session.level} expanded /></div>
      <div className="voice-call-copy">
        <p className="voice-call-time">LIGAÇÃO · {Math.floor(elapsed / 60000).toString().padStart(2, "0")}:{Math.floor(elapsed / 1000 % 60).toString().padStart(2, "0")}</p>
        <p role="status" className="voice-call-status">{meter}{voicePhaseLabels[session.phase]}</p>
        {!compact && <p className="voice-call-caption">{session.transcript ? `${session.speaker === "user" ? "Você" : "Jarvito"}: ${session.transcript}` : "Fale naturalmente. Estou aqui."}</p>}
      </div>
      <div className="voice-call-actions">
        <Hint content={microphoneLabel}><Button type="button" size="icon" variant="secondary" className="cursor-pointer rounded-full" disabled={session.phase === "closing" || session.phase === "preparing"} aria-label={microphoneLabel} onClick={() => { void control(interrupting ? "interrupt" : session.muted ? "unmute" : "mute"); }}>{session.muted ? <MicOff /> : session.phase === "speaking" ? <Volume2 /> : <Mic />}</Button></Hint>
        <Hint content="Encerrar ligação"><Button type="button" size="icon" variant="destructive" className="cursor-pointer rounded-full" aria-label="Encerrar ligação" onClick={() => { void control("end"); }}><PhoneOff /></Button></Hint>
      </div>
    </CardContent>
  </Card>;
}
