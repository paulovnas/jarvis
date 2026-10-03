import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { companionItemKey, type CompanionItem } from "@/core/companion";
import { useVoice } from "@/hooks/use-voice";
import type { CompanionNotice } from "./use-companion-notices";

const phrases = {
  completed: [
    "Pronto! {task} terminou. A atividade está aqui para você conferir.",
    "Boa notícia: {task} foi concluída. Pode dar uma olhada no resultado.",
    "Terminei {task}. O resultado já está disponível.",
    "Tudo certo com {task}. Quando quiser, confira o que ficou pronto.",
    "Mais uma pronta: {task}. O resultado está esperando por você.",
    "Concluí {task}. Podemos seguir para a próxima quando você quiser.",
  ],
  failed: [
    "Oh, tivemos um problema em {task}. Melhor você dar uma olhada.",
    "Preciso te avisar: {task} encontrou um erro. Veja a atividade comigo.",
    "Algo deu errado em {task}. Os detalhes estão na atividade.",
    "{task} não conseguiu terminar. Pode conferir o que aconteceu?",
    "Temos um imprevisto em {task}. Abra a atividade para ver os detalhes.",
    "{task} parou com um problema. Vamos dar uma olhada no que aconteceu?",
  ],
  question: [
    "Preciso da sua resposta em {task}. Pode me ajudar?",
    "Uma pergunta rápida sobre {task}. Sua resposta vai me ajudar a continuar.",
    "{task} está esperando uma resposta sua. A pergunta está aqui na ilha.",
    "Pode me dar uma orientação sobre {task}? Preciso da sua resposta para seguir.",
    "Tenho uma pergunta para você em {task}. Vamos resolver?",
    "Para continuar {task}, preciso te perguntar uma coisa. Dê uma olhada aqui.",
  ],
  approval: [
    "{task} precisa de uma aprovação sua. Abra a atividade para decidir.",
    "Preciso que você confira uma decisão em {task}. Ela está esperando sua aprovação.",
    "Uma ação em {task} aguarda sua aprovação. Os controles estão na conversa.",
    "Pode conferir {task}? Preciso da sua aprovação para continuar.",
    "Tem uma decisão esperando por você em {task}. Abra a atividade para revisar.",
    "{task} precisa do seu aval. Dê uma olhada nos controles da conversa.",
  ],
} as const;

/** Fixed speech is feedback only: never invent a result, read logs or call an LLM. */
function noticeKind(item: CompanionItem) {
  return item.status === "waiting" ? item.pendingQuestion ? "question" : "approval"
    : item.status === "completed" || item.status === "failed" ? item.status : null;
}
export function companionSpeechText(item: CompanionItem, variant: number): string | null {
  const kind = noticeKind(item);
  if (!kind || item.acknowledged) return null;
  const title = item.global ? "nossa conversa" : item.title.replace(/\s+/gu, " ").trim().slice(0, 180) || "esta atividade";
  return phrases[kind][variant % phrases[kind].length].replace("{task}", title);
}
const eventId = (item: CompanionItem) => item.status === "waiting"
  ? `${companionItemKey(item)}/waiting/${item.pendingQuestion?.turnId ?? item.attentionId}/${item.pendingQuestion?.toolId ?? "approval"}`
  : `${companionItemKey(item)}/${item.status}/${item.attentionId}`;
type Playback = { noticeId: string; sessionId: string | null; presented: boolean };
const isVolume = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= 1;

/** The visual queue owns ordering; reveal its current notice when that session can speak. */
export function useCompanionSpeech(items: CompanionItem[], loaded: boolean, visible = true, notice: CompanionNotice | null = null) {
  const voice = useVoice();
  const { start, control } = voice;
  const [enabled, setEnabled] = useState(false);
  const [ready, setReady] = useState(false);
  const [volume, setVolumeValue] = useState(1);
  const [volumeReady, setVolumeReady] = useState(false);
  const [volumeSaving, setVolumeSaving] = useState(false);
  const [volumeError, setVolumeError] = useState<string | null>(null);
  const [playback, setPlayback] = useState<Playback | null>(null);
  const currentPlayback = useRef<Playback | null>(null);
  const mounted = useRef(false);
  const saving = useRef(false);
  const savingVolume = useRef(false);
  const enabledRef = useRef(false);
  const [initialized, setInitialized] = useState(false);
  const [seen, setSeen] = useState(() => new Set<string>());
  const seenEvents = useRef(new Set<string>());
  const [variant, setVariant] = useState(() => Math.floor(Math.random() * 6));

  useEffect(() => {
    mounted.current = true;
    let version = 0;
    const refresh = async () => {
      const request = ++version;
      const current = () => mounted.current && request === version;
      await Promise.allSettled([
        invoke<unknown>("get_companion_speech").then(value => {
          if (!current()) return;
          enabledRef.current = value === true; setEnabled(value === true);
        }).finally(() => { if (current()) setReady(true); }),
        invoke<unknown>("get_companion_speech_volume").then(value => {
          if (!isVolume(value)) throw new Error("Invalid speech volume");
          if (!current()) return;
          setVolumeValue(value); setVolumeReady(true); setVolumeError(null);
        }).catch(() => { if (current()) setVolumeError("Não foi possível carregar o volume da voz."); }),
      ]);
    };
    const subscription = listen("companion:speech_changed", () => { void refresh(); });
    void subscription.then(() => refresh()).catch(() => { void refresh(); });
    return () => {
      mounted.current = false; ++version;
      const current = currentPlayback.current; currentPlayback.current = null;
      if (current?.sessionId) void control(current.sessionId, "end").catch(() => {});
      void subscription.then(stop => stop()).catch(() => {});
    };
  }, [control]);

  const toggle = useCallback(async () => {
    if (!ready || saving.current) return;
    saving.current = true;
    try {
      const next = !enabledRef.current;
      const value = await invoke<unknown>("set_companion_speech", { enabled: next });
      if (value !== next) throw new Error("Não foi possível salvar a preferência de fala.");
      if (!mounted.current) return;
      enabledRef.current = next; setEnabled(next);
    } finally { saving.current = false; }
  }, [ready]);

  const setVolume = useCallback(async (next: number) => {
    if (!isVolume(next)) throw new Error("O volume da voz deve estar entre 0 e 100%.");
    if (!volumeReady || savingVolume.current) return;
    savingVolume.current = true; setVolumeSaving(true); setVolumeError(null);
    try {
      const value = await invoke<unknown>("set_companion_speech_volume", { volume: next });
      if (!isVolume(value)) throw new Error("Não foi possível salvar o volume da voz.");
      if (mounted.current) setVolumeValue(value);
    } catch (cause) {
      if (mounted.current) setVolumeError("Não foi possível salvar o volume da voz.");
      throw cause;
    } finally {
      savingVolume.current = false;
      if (mounted.current) setVolumeSaving(false);
    }
  }, [volumeReady]);

  const id = notice ? eventId(notice.item) : null;
  const text = notice ? companionSpeechText(notice.item, variant) : null;
  const kind = notice ? noticeKind(notice.item) : null;
  const clips = kind ? voice.settings?.announcementClips.filter(id => new RegExp(`^${kind}-[1-8]$`, "u").test(id)).sort() : [];
  const clip = clips?.length ? clips[variant % clips.length] : undefined;
  const canSynthesize = Boolean(voice.settings?.config.enabled && voice.settings.speechReady);
  // Availability must not depend on the queue effect having selected a notice yet.
  const hasClips = voice.settings?.announcementClips.some(id => /^(completed|failed|question|approval)-[1-8]$/u.test(id));
  const canSpeak = ready && enabled && visible && Boolean(hasClips || canSynthesize)
    && (!voice.active || voice.session?.mode === "announcement");
  const shouldStart = initialized && canSpeak && Boolean((clip || canSynthesize) && id && text && !seen.has(id));
  const current = playback?.noticeId === notice?.id ? playback : null;
  const sessionMatches = Boolean(current?.sessionId && current.sessionId === voice.session?.id);
  const presented = current?.presented || sessionMatches && voice.session?.phase === "speaking";

  useEffect(() => {
    if (!loaded) return;
    // Seed loaded history, and never replay events received while another voice mode owns audio.
    const ids = !initialized || !canSpeak ? items.map(eventId) : [];
    for (const id of ids) seenEvents.current.add(id);
    while (seenEvents.current.size > 256) { const first = seenEvents.current.values().next().value; if (first) seenEvents.current.delete(first); }
    queueMicrotask(() => {
      if (!mounted.current) return;
      setInitialized(true);
      setSeen(current => {
        if (ids.every(id => current.has(id))) return current;
        const next = new Set([...current, ...ids]);
        while (next.size > 256) { const first = next.values().next().value; if (first) next.delete(first); }
        return next;
      });
    });
  }, [items, loaded, canSpeak, initialized]);

  useEffect(() => {
    const current = currentPlayback.current;
    if (!current) return;
    const ended = current.sessionId && voice.session && (voice.session.id !== current.sessionId || !voice.active || voice.session.phase === "closing");
    if (current.noticeId !== notice?.id || !canSpeak || ended) {
      currentPlayback.current = null;
      if (!ended && current.sessionId) void control(current.sessionId, "end").catch(() => {});
      queueMicrotask(() => { if (mounted.current) setPlayback(value => value === current ? null : value); });
    } else if (!current.presented && current.sessionId === voice.session?.id && voice.session?.phase === "speaking") {
      const next = { ...current, presented: true }; currentPlayback.current = next;
      queueMicrotask(() => { if (mounted.current && currentPlayback.current === next) setPlayback(next); });
    }
  }, [notice?.id, canSpeak, voice.session, voice.active, playback?.sessionId, control]);

  useEffect(() => {
    if (!shouldStart || !id || !text || !notice || currentPlayback.current || voice.active || seenEvents.current.has(id)) return;
    const request: Playback = { noticeId: notice.id, sessionId: null, presented: false };
    currentPlayback.current = request; seenEvents.current.add(id);
    queueMicrotask(() => {
      if (!mounted.current) return;
      setSeen(current => new Set([...current, id])); setVariant(current => current + 1);
      if (currentPlayback.current === request) setPlayback(request);
    });
    const requestAudio = clip ? start("companion-notice", "announcement", text, clip) : start("companion-notice", "announcement", text);
    void requestAudio.then(session => {
      if (!mounted.current || currentPlayback.current !== request) {
        if (session.id) void control(session.id, "end").catch(() => {});
        return;
      }
      const next = { ...request, sessionId: session.id }; currentPlayback.current = next;
      setPlayback(next);
    }).catch(() => {
      if (currentPlayback.current !== request) return;
      currentPlayback.current = null;
      if (mounted.current) setPlayback(null); // Visual fallback; never retry an uncertain speech outcome.
    });
  }, [shouldStart, id, text, clip, notice, voice.active, start, control]);

  return { enabled, ready, toggle, volume, volumeReady, volumeSaving, volumeError, setVolume,
    pending: canSpeak && (shouldStart || Boolean(current && !presented)), playing: canSpeak && Boolean(current) };
}
