import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { z } from "zod";
import { toast } from "sonner";
import { libraryError } from "@/core/library";
import { modelChoiceSchema } from "@/core/workflow-catalog";
import type { AgentModelConfig, ModelChoice } from "./use-agent-models";

const configSchema = z.record(z.string(), modelChoiceSchema);

export function useChatAgentModels(conversationId: string) {
  const [stored, setStored] = useState<{ conversationId: string; data: AgentModelConfig } | null>(null);
  const [failure, setFailure] = useState<{ conversationId: string; message: string } | null>(null);
  const data = stored?.conversationId === conversationId ? stored.data : null;
  const error = failure?.conversationId === conversationId ? failure.message : null;
  const [saving, setSaving] = useState(false);
  const version = useRef(0);
  const flight = useRef(false);
  const refresh = useCallback(async () => {
    const request = ++version.current;
    try {
      const config = configSchema.parse(await invoke("get_chat_agent_models", { conversationId }));
      if (request === version.current) { setStored({ conversationId, data: config }); setFailure(null); }
    } catch (cause) {
      if (request === version.current) setFailure({ conversationId, message: libraryError(cause, "Não foi possível carregar os modelos deste chat.") });
    }
  }, [conversationId]);
  useEffect(() => {
    let active = true; const disposers: (() => void)[] = [];
    void Promise.all([
      listen<{ conversationId: string }>("chat-agent-models:changed", event => { if (active && event.payload.conversationId === conversationId) void refresh(); }),
      listen("provider-model-bindings:changed", () => { if (active) void refresh(); }),
    ].map(promise => promise.then(stop => { if (active) disposers.push(stop); else stop(); })))
      .then(() => { if (active) void refresh(); }).catch(() => { if (active) void refresh(); });
    return () => { active = false; version.current += 1; disposers.forEach(stop => stop()); };
  }, [conversationId, refresh]);
  const save = async (key: string, choice: ModelChoice) => {
    if (flight.current) return false;
    flight.current = true; setSaving(true);
    const request = ++version.current;
    try {
      const config = configSchema.parse(await invoke("set_chat_agent_model", { conversationId, key, choice }));
      if (request === version.current) { setStored({ conversationId, data: config }); setFailure(null); }
      return true;
    } catch (cause) {
      toast.error(libraryError(cause, "Não foi possível salvar o modelo deste chat.")); return false;
    } finally { flight.current = false; setSaving(false); }
  };
  return { data, error, saving, save, refresh };
}

export type ChatAgentModelsController = ReturnType<typeof useChatAgentModels>;
