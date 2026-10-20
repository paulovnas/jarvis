import { createContext, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { pixelArtResultSchema, type PixelArtResult } from "@/core/pixel-art";

export const PixelArtResultsContext = createContext<Record<string, PixelArtResult>>({});

export function usePixelArtResults(conversationId: string, enabled = true) {
  const [state, setState] = useState<{ conversationId: string; jobs: Record<string, PixelArtResult> }>({ conversationId, jobs: {} });
  useEffect(() => {
    if (!enabled) return;
    let active = true;
    const accept = (value: unknown) => {
      const parsed = pixelArtResultSchema.safeParse(value);
      if (!active || !parsed.success || parsed.data.conversationId !== conversationId) return;
      const job = parsed.data;
      setState(previous => {
        const jobs = previous.conversationId === conversationId ? previous.jobs : {};
        if ((jobs[job.jobId]?.revision ?? -1) >= job.revision) return previous;
        return { conversationId, jobs: { ...jobs, [job.jobId]: job } };
      });
    };
    const subscription = listen<unknown>("pixel-art-progress", event => accept(event.payload));
    void subscription.then(async () => {
      if (!active) return;
      const value = await invoke<unknown>("get_pixel_art_jobs", { conversationId });
      if (Array.isArray(value)) value.forEach(accept);
    }).catch(() => undefined);
    return () => {
      active = false;
      void subscription.then(unlisten => unlisten()).catch(() => undefined);
    };
  }, [conversationId, enabled]);
  return state.conversationId === conversationId ? state.jobs : {};
}
