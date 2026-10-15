import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { z } from "zod";
import type { ChatSnapshot } from "@/core/chat";

const predictionSchema = z.object({ turnId: z.string().min(1), text: z.string().trim().min(1).max(4096) });
const changedSchema = z.object({ conversationId: z.string() });
type Prediction = z.infer<typeof predictionSchema>;

export function useMessagePrediction(conversationId: string | undefined, snapshot: ChatSnapshot | null | undefined, occupied: boolean, paused = false) {
  const latest = snapshot?.turns[snapshot.turns.length - 1];
  const delivery = snapshot && snapshot.conversationId === conversationId && !snapshot.activeTurnId && latest?.status === "completed" ? latest.id : null;
  const [prediction, setPrediction] = useState<Prediction | null>(null);
  if (prediction && (occupied || paused || prediction.turnId !== delivery)) setPrediction(null);
  const current = useRef({ conversationId, delivery, occupied, paused });
  const dismissed = useRef<string | null>(null);
  useEffect(() => { current.current = { conversationId, delivery, occupied, paused }; });

  const suppress = useCallback(() => {
    const value = current.current;
    if (!value.conversationId || !value.delivery || dismissed.current === value.delivery) return;
    dismissed.current = value.delivery;
    void invoke("dismiss_chat_prediction", { conversationId: value.conversationId, turnId: value.delivery }).catch(() => {});
  }, []);
  const dismiss = useCallback(() => { setPrediction(null); suppress(); }, [suppress]);

  useEffect(() => {
    let alive = true;
    let version = 0;
    let unlisten: (() => void) | undefined;
    const refresh = async () => {
      const responseVersion = ++version;
      const value = current.current;
      try {
        const available = value.conversationId && value.delivery && !value.occupied && !value.paused && dismissed.current !== value.delivery;
        const parsed = predictionSchema.nullable().safeParse(await (available ? invoke("get_chat_prediction", { conversationId: value.conversationId }) : null));
        if (!alive || responseVersion !== version) return;
        const latest = current.current;
        setPrediction(parsed.success && parsed.data && parsed.data.turnId === latest.delivery && !latest.occupied && !latest.paused && dismissed.current !== latest.delivery ? parsed.data : null);
      } catch { if (alive && responseVersion === version) setPrediction(null); }
    };
    const start = async () => {
      const stop = await listen("chat:prediction_changed", event => {
        const changed = changedSchema.safeParse(event.payload);
        if (changed.success && changed.data.conversationId === conversationId) void refresh();
      });
      if (!alive) { stop(); return; }
      unlisten = stop;
      await refresh();
    };
    if (conversationId && delivery && !paused) void start().catch(() => {});
    else void refresh();
    return () => { alive = false; unlisten?.(); };
  }, [conversationId, delivery, paused]);

  useEffect(() => { if (occupied) suppress(); }, [occupied, delivery, suppress]);
  return { text: !occupied && !paused && prediction && prediction.turnId === delivery ? prediction.text : undefined, dismiss };
}
