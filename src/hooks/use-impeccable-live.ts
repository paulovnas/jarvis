import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { z } from "zod";
import { toast } from "sonner";
import type { TurnOptions } from "@/core/chat";
import { libraryError } from "@/core/library";

const taskSchema = z.object({
  requestId: z.string(),
  actionId: z.string(),
  revision: z.number().int().nonnegative(),
  content: z.string(),
  status: z.enum(["pending", "paused", "running", "completed", "cancelled", "blocked"]),
  beadId: z.string().nullable(),
  turnId: z.string().nullable(),
  createdAt: z.number().int().nonnegative(),
});
const liveSchema = z.object({
  conversationId: z.string(),
  state: z.enum(["off", "starting", "ready", "setup"]),
  url: z.string().nullable(),
  tabId: z.string().nullable(),
  error: z.string().nullable(),
  setupNeeded: z.record(z.string(), z.unknown()).nullable(),
  phase: z.enum(["idle", "connecting", "waiting", "running", "blocked"]).default("idle"),
  actionId: z.string().nullable().default(null),
  model: z.string().nullable().optional(),
  provider: z.string().nullable().optional(),
  step: z.number().int().nonnegative().nullable().optional(),
  activity: z.string().nullable().optional(),
  elapsedMs: z.number().nonnegative().optional(),
  queueCount: z.number().int().nonnegative().optional(),
  tasks: z.array(taskSchema).default([]),
});
const actionSchema = z.object({ conversationId: z.string(), tabId: z.string(), actionId: z.string().nullable(), requestId: z.string().min(1).max(128), action: z.enum(["cancel", "resume"]), revision: z.number().int().nonnegative() });
type Recovery = { options?: TurnOptions; url?: string; tabId?: string; canResume: boolean; blockedReason?: string };
export type ImpeccableLiveTask = z.infer<typeof taskSchema>;
type LiveStatus = z.infer<typeof liveSchema>;
const off = (conversationId: string): LiveStatus => ({ conversationId, state: "off", url: null, tabId: null, error: null, setupNeeded: null, phase: "idle", actionId: null, tasks: [] });
function localPage(value?: string) {
  try {
    const url = new URL(value ?? "");
    return ["http:", "https:"].includes(url.protocol) && !url.username && !url.password && (url.hostname === "localhost" || url.hostname === "[::1]" || /^127\.\d+\.\d+\.\d+$/.test(url.hostname)) ? url.href : undefined;
  } catch { return undefined; }
}

export function useImpeccableLive(conversationId: string, enabled = true, recovery?: Recovery) {
  const [status, setStatus] = useState(() => off(conversationId));
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const operation = useRef(false);
  const generation = useRef({ version: 0, conversationId, alive: true });
  const actionHandler = useRef<(value: unknown) => Promise<void>>(async () => {});
  const lastNative = useRef<LiveStatus | null>(null);
  const syncedSelection = useRef<string | null>(null);
  useEffect(() => {
    const epoch = generation.current;
    epoch.conversationId = conversationId;
    epoch.alive = true;
    if (!enabled) return;
    let alive = true;
    const request = ++epoch.version;
    const subscription = listen<unknown>("impeccable-live:changed", event => {
      const next = liveSchema.safeParse(event.payload);
      if (!alive || !next.success || next.data.conversationId !== conversationId) return;
      ++epoch.version;
      lastNative.current = next.data;
      setStatus(next.data);
      setLoaded(true);
      const setupError = next.data.setupNeeded?.error;
      const recoverableSetup = next.data.state === "setup" && typeof setupError === "string" && ["config_missing", "context_missing", "target_selection_required", "page_unavailable", "dev_server_missing"].includes(setupError);
      if (next.data.error && !recoverableSetup) toast.error("Modo Live", { id: `impeccable-live:${conversationId}`, description: next.data.error });
    });
    const actions = listen<unknown>("impeccable-live:action", event => { if (alive) void actionHandler.current(event.payload); });
    void invoke<unknown>("get_impeccable_live", { conversationId }).then(value => {
      if (!alive || epoch.version !== request) return;
      const next = liveSchema.parse(value);
      if (next.conversationId !== conversationId) throw new Error("A sessão Live pertence a outra conversa.");
      setStatus(next); setLoaded(true);
    }).catch(cause => {
      if (!alive || epoch.version !== request) return;
      setLoaded(true);
      const error = libraryError(cause, "Não foi possível consultar o modo Live.");
      setStatus({ ...off(conversationId), error, phase: "blocked" });
      toast.error(error);
    });
    return () => { alive = false; epoch.alive = false; ++epoch.version; for (const request of [subscription, actions]) void request.then(unlisten => unlisten()).catch(() => {}); };
  }, [conversationId, enabled]);
  const activeStatus = status.conversationId === conversationId ? status : off(conversationId);
  const active = activeStatus.state !== "off";
  useEffect(() => {
    if (!enabled || activeStatus.state !== "ready") { syncedSelection.current = null; return; }
    if (!recovery?.options) return;
    const key = JSON.stringify([conversationId, recovery.options]);
    if (syncedSelection.current === key) return;
    syncedSelection.current = key;
    void invoke("set_impeccable_live_options", { conversationId, options: recovery.options }).catch(cause => {
      if (generation.current.alive && generation.current.conversationId === conversationId && syncedSelection.current === key) {
        toast.error(libraryError(cause, "Não foi possível atualizar o modelo do Live. Retome o Live para tentar novamente."));
      }
    });
  });
  async function change(start: boolean, options?: TurnOptions, url?: string) {
    if (!enabled || operation.current || (start && !options)) return "Aguarde a ação atual ou selecione um modelo disponível no chat.";
    if (start && activeStatus.state === "setup" && ["waiting", "running"].includes(activeStatus.phase)) return "O Designer já está preparando o modo Live.";
    operation.current = true; setBusy(true);
    const epoch = generation.current;
    const request = ++epoch.version;
    try {
      const page = localPage(url);
      const next = liveSchema.parse(await invoke<unknown>(start ? "start_impeccable_live" : "stop_impeccable_live", {
        conversationId, ...(start ? { options, ...(page ? { url: page } : {}) } : {}),
      }));
      if (epoch.alive && epoch.version === request && epoch.conversationId === conversationId && next.conversationId === conversationId) setStatus(next);
    } catch (cause) {
      const error = libraryError(cause, start ? "Não foi possível iniciar o modo Live." : "Não foi possível desligar o modo Live.");
      const successful = epoch.version !== request && lastNative.current?.conversationId === conversationId && !lastNative.current.error && (lastNative.current.state === "ready" || lastNative.current.state === "off" || (start && lastNative.current.state === "setup" && ["waiting", "running"].includes(lastNative.current.phase)));
      if (epoch.alive && epoch.conversationId === conversationId) setStatus(current => {
        if (epoch.version !== request && current.conversationId === conversationId && !current.error && (current.state === "ready" || current.state === "off" || (start && current.state === "setup" && ["waiting", "running"].includes(current.phase)))) return current;
        const previous = current.conversationId === conversationId ? current : off(conversationId);
        return { ...previous, state: previous.state === "starting" ? "setup" : previous.state, error, phase: "blocked" };
      });
      if (!successful) toast.error(error);
      return successful ? null : error;
    } finally { operation.current = false; setBusy(false); }
    return null;
  }
  async function cancelAction(actionId: string, requestId?: string, expectedRevision?: number) {
    if (operation.current) return "Aguarde a ação Live atual terminar.";
    operation.current = true; setBusy(true);
    const epoch = generation.current;
    const request = ++epoch.version;
    try {
      const next = liveSchema.parse(await invoke<unknown>("cancel_impeccable_live_action", {
        conversationId, actionId, ...(requestId !== undefined ? { requestId } : {}),
        ...(expectedRevision !== undefined ? { expectedRevision } : {}),
      }));
      if (epoch.alive && epoch.version === request && epoch.conversationId === conversationId && next.conversationId === conversationId) setStatus(next);
      return null;
    } catch (cause) {
      const error = libraryError(cause, "Não foi possível interromper a alteração Live.");
      if (epoch.alive && epoch.conversationId === conversationId) toast.error(error);
      return error;
    }
    finally { operation.current = false; setBusy(false); }
  }
  async function taskAction(command: string, task: ImpeccableLiveTask, content?: string, options?: TurnOptions) {
    if (!enabled || operation.current) return "Aguarde a ação Live atual terminar.";
    operation.current = true; setBusy(true);
    const epoch = generation.current;
    const request = ++epoch.version;
    try {
      const next = liveSchema.parse(await invoke<unknown>(command, {
        conversationId, requestId: task.requestId, expectedRevision: task.revision,
        ...(content !== undefined ? { content } : {}),
        ...(options ? { options } : {}),
      }));
      if (epoch.alive && epoch.version === request && epoch.conversationId === conversationId && next.conversationId === conversationId) setStatus(next);
      return null;
    } catch (cause) {
      const error = libraryError(cause, "Não foi possível atualizar esta alteração Live.");
      if (epoch.alive && epoch.conversationId === conversationId) toast.error(error);
      return error;
    } finally { operation.current = false; setBusy(false); }
  }
  useEffect(() => {
    actionHandler.current = async value => {
      const parsed = actionSchema.safeParse(value);
      if (!parsed.success) return;
      const request = parsed.data;
      if (!enabled || request.conversationId !== conversationId || request.tabId !== activeStatus.tabId || request.actionId !== activeStatus.actionId) return;
      let error: string | null;
      if (request.action === "cancel") error = request.actionId && ["waiting", "running"].includes(activeStatus.phase) ? await cancelAction(request.actionId) : "Esta alteração Live não está mais em execução.";
      else error = !recovery?.canResume || !recovery.options ? recovery?.blockedReason ?? "Selecione um modelo disponível no chat para retomar o Live." : await change(true, recovery.options, recovery.tabId === activeStatus.tabId ? recovery.url ?? activeStatus.url ?? undefined : activeStatus.url ?? undefined);
      if (!generation.current.alive || generation.current.conversationId !== conversationId) return;
      await invoke("impeccable_live_action_feedback", { conversationId, tabId: request.tabId, requestId: request.requestId, error }).catch(() => {});
    };
  });
  const toggle = async (options?: TurnOptions, url?: string) => { await change(!active, options, url); };
  const resume = async (options: TurnOptions, url?: string) => { await change(true, options, url); };
  const editTask = (task: ImpeccableLiveTask, content: string) => taskAction("edit_impeccable_live_task", task, content);
  const removeTask = (task: ImpeccableLiveTask) => taskAction("remove_impeccable_live_task", task);
  const resumeTask = (task: ImpeccableLiveTask) => {
    if (!recovery?.canResume || !recovery.options) return Promise.resolve(recovery?.blockedReason ?? "Selecione um modelo disponível no chat para retomar esta alteração.");
    return taskAction("resume_impeccable_live_task", task, undefined, recovery.options);
  };
  return { status: activeStatus, active, loaded: loaded && status.conversationId === conversationId, busy: busy || activeStatus.state === "starting", toggle, resume, cancelAction, editTask, removeTask, resumeTask };
}
export type ImpeccableLiveController = ReturnType<typeof useImpeccableLive>;
