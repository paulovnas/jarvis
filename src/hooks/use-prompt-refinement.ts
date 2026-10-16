import { useCallback, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import type { ChatDraft } from "@/core/chat";
import { libraryError } from "@/core/library";
import { refinePrompt } from "@/core/prompt-refinement";

interface Job { id: string }
interface Entry { job: Job | null; listeners: Set<() => void> }
const owners = new WeakMap<Map<string, ChatDraft>, Map<string, Entry>>();

function setJob(entry: Entry, job: Job | null) {
  entry.job = job;
  for (const listener of entry.listeners) listener();
}

function entryFor(drafts: Map<string, ChatDraft>, key: string): Entry {
  let entries = owners.get(drafts);
  if (!entries) { entries = new Map(); owners.set(drafts, entries); }
  let entry = entries.get(key);
  if (!entry) { entry = { job: null, listeners: new Set() }; entries.set(key, entry); }
  return entry;
}

export function usePromptRefinement(drafts: Map<string, ChatDraft>, key: string, conversationId?: string) {
  const entry = entryFor(drafts, key);
  const subscribe = useCallback((listener: () => void) => {
    entry.listeners.add(listener);
    return () => { entry.listeners.delete(listener); };
  }, [entry]);
  const snapshot = useCallback(() => entry.job, [entry]);
  const job = useSyncExternalStore(subscribe, snapshot);
  return {
    refining: job !== null,
    pending: () => entry.job !== null,
    start: async (prepare: () => Promise<void>, read: () => ChatDraft) => {
      if (entry.job) return;
      const request = { id: crypto.randomUUID() };
      const active = () => entry.job === request;
      setJob(entry, request);
      try {
        await prepare();
        if (!active()) return;
        const submitted = read();
        drafts.set(key, submitted);
        const refined = await refinePrompt(submitted, conversationId, request.id);
        if (!active()) return;
        if (drafts.get(key) !== submitted) {
          toast.info("O rascunho mudou durante o refinamento. Seu texto atual foi mantido.");
          return;
        }
        // The origin draft store outlives the composer when navigating to another chat.
        drafts.set(key, refined);
      } catch (cause) {
        if (active()) toast.error(libraryError(cause, "Não foi possível refinar o prompt. Seu texto foi mantido."));
      } finally {
        if (active()) setJob(entry, null);
      }
    },
    cancel: () => {
      const request = entry.job;
      if (!request) return;
      setJob(entry, null);
      void invoke("cancel_chat_prompt_refinement", { requestId: request.id }).catch(() => {
        toast.error("O resultado foi descartado, mas não foi possível confirmar a interrupção do refinamento.");
      });
    },
  };
}
