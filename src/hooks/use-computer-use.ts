import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { z } from "zod";
import { onDesktopResume } from "@/core/desktop-resume";
import { computerAppSchema, computerCapabilitiesSchema, computerSnapshotSchema, computerUpdateSchema, computerUseError, type ComputerAccess, type ComputerApp, type ComputerMode, type ComputerPermission, type ComputerSnapshot } from "@/core/computer-use";

type Scoped<T> = { conversationId: string | null; value: T };

export function useComputerUse(conversationId: string | null = null, enabled = true) {
  const [state, setState] = useState<Scoped<ComputerSnapshot> | null>(null);
  const [inventory, setInventory] = useState<Scoped<ComputerApp[]> | null>(null);
  const [failure, setFailure] = useState<Scoped<string> | null>(null);
  const [pending, setPending] = useState<Scoped<string> | null>(null);
  const generation = useRef(0);
  const sequence = useRef(0);
  const version = useRef(0);
  const inFlight = useRef<number | null>(null);

  const run = useCallback(async (command: string, args: Record<string, unknown>, interrupt = false): Promise<boolean> => {
    if (!enabled || (inFlight.current !== null && !interrupt)) return false;
    const request = generation.current;
    const operation = ++sequence.current;
    const before = command === "list_computer_apps" ? version.current : ++version.current;
    inFlight.current = operation;
    setPending({ conversationId, value: command });
    setFailure(null);
    try {
      const result = await invoke<unknown>(command, args);
      if (request !== generation.current || operation !== sequence.current) return false;
      if (command === "list_computer_apps") {
        setInventory({ conversationId, value: z.array(computerAppSchema).parse(result) });
      } else if (command === "request_computer_permission") {
        const capabilities = computerCapabilitiesSchema.parse(result);
        if (before === version.current) setState(current => current?.conversationId === conversationId ? { conversationId, value: { ...current.value, capabilities } } : current);
      } else {
        const next = computerSnapshotSchema.parse(result);
        if (before === version.current) setState({ conversationId, value: next });
      }
      return true;
    } catch (cause) {
      if (request === generation.current && operation === sequence.current) setFailure({ conversationId, value: computerUseError(cause) });
      return false;
    } finally {
      if (request === generation.current && inFlight.current === operation) { inFlight.current = null; setPending(null); }
    }
  }, [conversationId, enabled]);
  const refresh = useCallback(() => run("get_computer_use_status", { conversationId }), [conversationId, run]);

  useEffect(() => {
    ++generation.current;
    inFlight.current = null;
    if (!enabled) return;
    const request = generation.current;
    const before = ++version.current;
    const subscription = conversationId === null ? null : listen("computer-use-updated", event => {
      if (request !== generation.current) return;
      const next = computerUpdateSchema.safeParse(event.payload);
      if (next.success && next.data.conversationId === conversationId) {
        ++version.current;
        setState({ conversationId, value: next.data.snapshot });
        setFailure(null);
      } else if (!next.success && typeof event.payload === "object" && event.payload !== null && Reflect.get(event.payload, "conversationId") === conversationId) {
        setFailure({ conversationId, value: computerUseError(next.error) });
      }
    });
    void subscription?.catch(cause => { if (request === generation.current) setFailure({ conversationId, value: computerUseError(cause) }); });
    void invoke<unknown>("get_computer_use_status", { conversationId }).then(result => {
      if (request !== generation.current || before !== version.current) return;
      setState({ conversationId, value: computerSnapshotSchema.parse(result) });
      setFailure(null);
    }).catch(cause => { if (request === generation.current && before === version.current) setFailure({ conversationId, value: computerUseError(cause) }); });
    const stopResume = onDesktopResume(() => { void refresh(); });
    const modeSubscription = listen("computer-use-mode-changed", () => { if (request === generation.current) void refresh(); });
    return () => {
      generation.current = request + 1;
      inFlight.current = null;
      stopResume();
      void subscription?.then(stop => stop()).catch(() => {});
      void modeSubscription.then(stop => stop()).catch(() => {});
    };
  }, [conversationId, enabled, refresh]);

  return {
    snapshot: enabled && state?.conversationId === conversationId ? state.value : null,
    apps: inventory?.conversationId === conversationId ? inventory.value : null,
    error: failure?.conversationId === conversationId ? failure.value : null,
    busy: pending?.conversationId === conversationId ? pending.value : null,
    refresh,
    loadApps: () => run("list_computer_apps", {}),
    requestPermission: (permission: ComputerPermission) => run("request_computer_permission", { permission }),
    setMode: (mode: ComputerMode) => run("set_computer_use_mode", { mode }),
    authorize: (appId: string, access: ComputerAccess) => conversationId ? run("authorize_computer_app", { conversationId, appId, access }) : Promise.resolve(false),
    revoke: (grantId: string) => conversationId ? run("revoke_computer_app", { conversationId, grantId }) : Promise.resolve(false),
    stop: () => conversationId ? run("stop_computer_use", { conversationId }, true) : Promise.resolve(false),
  };
}
