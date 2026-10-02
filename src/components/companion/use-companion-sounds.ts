import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { companionItemKey, type CompanionItem } from "@/core/companion";

export type CompanionSound = "open" | "close" | "hover" | "send" | "approve" | "question" | "work" | "finish" | "error" | "rate" | "think" | "search" | "poke" | "dizzy";
type Note = readonly [frequency: number, offset: number, duration: number, target?: number];
// Original quiet electronic cues. Envelopes prevent clicks at either end of a note.
const notes: Record<CompanionSound, readonly Note[]> = {
  open: [[392, 0, .12, 587], [784, .08, .12]], close: [[587, 0, .16, 294]],
  hover: [[880, 0, .07, 988]], send: [[440, 0, .12, 659]], approve: [[523, 0, .12], [784, .1, .16]],
  question: [[659, 0, .16], [880, .18, .2]], work: [[294, 0, .14, 392]],
  finish: [[523, 0, .14], [659, .12, .14], [784, .24, .25]],
  error: [[330, 0, .19, 262], [220, .2, .24]], rate: [[440, 0, .13], [440, .2, .18]],
  think: [[392, 0, .14, 440]], search: [[523, 0, .12], [587, .1, .12]],
  poke: [[740, 0, .1, 330]], dizzy: [[659, 0, .13, 440], [440, .12, .13, 587], [587, .24, .2, 330]],
};

const eventId = (item: CompanionItem) => item.status === "waiting"
  ? `${companionItemKey(item)}/waiting/${item.pendingQuestion?.turnId ?? item.attentionId}/${item.pendingQuestion?.toolId ?? "approval"}`
  : `${companionItemKey(item)}/${item.status}/${item.attentionId}`;
const cue = (item: CompanionItem): CompanionSound | null => {
  if (item.status === "waiting") return "question";
  if (item.acknowledged) return null;
  if (item.status === "failed") return "error";
  if (item.status === "completed") return "finish";
  if (item.status === "reconnecting") return "rate";
  return item.status === "running" ? "work" : null;
};
const priority: CompanionSound[] = ["question", "error", "finish", "rate", "work"];

/** Uses the native task projection, never chat text or external provider hooks. */
export function useCompanionSounds(items: CompanionItem[], loaded: boolean, visible = true) {
  const [enabled, setEnabled] = useState(false);
  const [ready, setReady] = useState(false);
  const enabledRef = useRef(false);
  const visibleRef = useRef(visible);
  const mounted = useRef(false);
  const context = useRef<AudioContext | null>(null);
  const oscillators = useRef(new Set<OscillatorNode>());
  const suspendTimer = useRef<number | undefined>(undefined);
  const lastSound = useRef({ name: "" as string, at: -Infinity });
  const initialized = useRef(false);
  const previous = useRef(new Map<string, string>());
  const seen = useRef(new Set<string>());
  const saving = useRef(false);
  useEffect(() => { visibleRef.current = visible; }, [visible]);

  useEffect(() => {
    mounted.current = true;
    let alive = true;
    const voices = oscillators.current;
    void invoke<unknown>("get_companion_sound").then(value => {
      if (!alive) return;
      enabledRef.current = value === true;
      setEnabled(value === true);
    }).catch(() => { /* Audio availability must never prevent interaction. */ }).finally(() => { if (alive) setReady(true); });
    return () => {
      alive = false;
      mounted.current = false;
      window.clearTimeout(suspendTimer.current);
      for (const oscillator of voices) { try { oscillator.stop(); } catch { /* Already ended. */ } }
      voices.clear();
      const current = context.current;
      context.current = null;
      if (current) void current.close().catch(() => {});
    };
  }, []);

  const play = useCallback((name: CompanionSound) => {
    if (!enabledRef.current || !visibleRef.current || !mounted.current) return;
    const now = performance.now();
    if (name !== "dizzy" && (now - lastSound.current.at < 100 || (name === "hover" && lastSound.current.name === "hover" && now - lastSound.current.at < 2000))) return;
    lastSound.current = { name, at: now };
    try {
      const audio = context.current ?? (context.current = new AudioContext());
      const render = () => {
        if (!mounted.current || !enabledRef.current || !visibleRef.current || audio.state !== "running") return;
        window.clearTimeout(suspendTimer.current);
        const start = audio.currentTime + .01;
        for (const [frequency, offset, duration, target] of notes[name]) {
          // Bound concurrent voices even when several projects finish together.
          if (oscillators.current.size >= 9) break;
          const oscillator = audio.createOscillator();
          const gain = audio.createGain();
          oscillator.type = "sine";
          oscillator.frequency.setValueAtTime(frequency, start + offset);
          if (target) oscillator.frequency.exponentialRampToValueAtTime(target, start + offset + duration);
          gain.gain.setValueAtTime(0, start + offset);
          gain.gain.linearRampToValueAtTime(.12, start + offset + .015);
          gain.gain.exponentialRampToValueAtTime(.001, start + offset + duration - .01);
          gain.gain.linearRampToValueAtTime(0, start + offset + duration);
          oscillator.connect(gain);
          gain.connect(audio.destination);
          oscillators.current.add(oscillator);
          oscillator.onended = () => { oscillator.disconnect(); gain.disconnect(); oscillators.current.delete(oscillator); };
          oscillator.start(start + offset);
          oscillator.stop(start + offset + duration);
        }
        suspendTimer.current = window.setTimeout(() => { void audio.suspend().catch(() => {}); }, 1500);
      };
      if (audio.state === "suspended") void audio.resume().then(render).catch(() => {});
      else render();
    } catch { /* Unsupported or blocked WebAudio is a silent fallback. */ }
  }, []);

  const toggle = useCallback(async () => {
    if (!ready || saving.current) return;
    saving.current = true;
    try {
      const next = !enabledRef.current;
      const value = await invoke<unknown>("set_companion_sound", { enabled: next });
      if (value !== next) throw new Error("Não foi possível salvar a preferência de som.");
      if (!mounted.current) return;
      enabledRef.current = next;
      setEnabled(next);
      if (!next) {
        for (const oscillator of oscillators.current) { try { oscillator.stop(); } catch { /* Already ended. */ } }
        if (context.current) void context.current.suspend().catch(() => {});
      } else play("approve");
    } finally { saving.current = false; }
  }, [play, ready]);

  useEffect(() => {
    if (!loaded) return;
    const candidates: CompanionSound[] = [];
    for (const item of items) {
      const key = companionItemKey(item);
      const id = eventId(item);
      const before = previous.current.get(key);
      const sound = cue(item);
      const changed = item.status === "running" || item.status === "reconnecting"
        ? before !== item.status : !seen.current.has(id);
      if (initialized.current && changed && sound) candidates.push(sound);
      previous.current.set(key, item.status);
      seen.current.add(id);
    }
    initialized.current = true;
    while (seen.current.size > 256) { const first = seen.current.values().next().value; if (first) seen.current.delete(first); }
    while (previous.current.size > 128) { const first = previous.current.keys().next().value; if (first) previous.current.delete(first); }
    const sound = priority.find(name => candidates.includes(name));
    if (sound) play(sound);
  }, [items, loaded, play]);

  return { enabled, ready, toggle, play };
}
