import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { toast } from "sonner";
import { libraryError } from "@/core/library";
import { DEFAULT_TERMINAL_PREFERENCES, systemSnapshotSchema, terminalFontFamily, type TerminalPreferences } from "@/core/system-preferences";
import { terminalOutputEventSchema, terminalSnapshotSchema, type ChatTerminal, type TerminalOutputEvent } from "@/core/terminals";

function terminalTheme(host: HTMLElement) {
  const styles = getComputedStyle(host);
  return {
    background: styles.getPropertyValue("--sidebar").trim(),
    foreground: styles.getPropertyValue("--foreground").trim(),
    cursor: styles.getPropertyValue("--primary").trim(),
    selectionBackground: styles.getPropertyValue("--secondary").trim(),
  };
}

export function TerminalSurface({ conversationId, terminal }: { conversationId: string; terminal: ChatTerminal }) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const element = host.current;
    if (!element) return;
    const initialAppearance: TerminalPreferences = DEFAULT_TERMINAL_PREFERENCES;
    const xterm = new Terminal({
      cursorBlink: terminal.status === "running",
      disableStdin: terminal.status !== "running",
      cursorStyle: "bar",
      fontFamily: terminalFontFamily(initialAppearance.fontFamily),
      fontSize: initialAppearance.fontSize,
      lineHeight: 1.3,
      letterSpacing: 0,
      scrollback: 5_000,
      theme: terminalTheme(element),
    });
    const fit = new FitAddon();
    xterm.loadAddon(fit);
    let disposed = false;
    let hydrated = false;
    let lastRevision = 0;
    let lastSize = "";
    let pendingInput = "";
    let pendingOutput: TerminalOutputEvent[] = [];
    let inputTimer: number | undefined;
    let unlistenOutput: (() => void) | undefined;
    let unlistenPreferences: (() => void) | undefined;
    let opened = false;
    let appearanceRevision = 0;
    const resize = () => {
      if (disposed || !element.clientWidth || !element.clientHeight) return;
      try {
        fit.fit();
      } catch {
        return;
      }
      const size = `${xterm.rows}:${xterm.cols}`;
      if (size === lastSize || xterm.rows < 2 || xterm.cols < 2) return;
      lastSize = size;
      void invoke("resize_chat_terminal", {
        conversationId,
        id: terminal.id,
        rows: xterm.rows,
        cols: xterm.cols,
      });
    };
    const applyAppearance = async (preferences: TerminalPreferences, refit: boolean) => {
      const revision = ++appearanceRevision;
      const fontFamily = terminalFontFamily(preferences.fontFamily);
      await document.fonts?.load(`${preferences.fontSize}px ${fontFamily}`).catch(() => undefined);
      if (disposed || revision !== appearanceRevision) return;
      xterm.options.fontFamily = fontFamily;
      xterm.options.fontSize = preferences.fontSize;
      if (opened && refit) {
        lastSize = "";
        resize();
      }
    };
    const observer = new ResizeObserver(resize);
    const input = terminal.status === "running"
      ? xterm.onData(data => {
          pendingInput += data;
          if (inputTimer) return;
          inputTimer = window.setTimeout(() => {
            const value = pendingInput;
            pendingInput = "";
            inputTimer = undefined;
            void invoke("write_chat_terminal", { conversationId, id: terminal.id, input: value }).catch(error => {
              toast.error(libraryError(error, "Não foi possível enviar dados ao terminal."));
            });
          }, 16);
        })
      : undefined;
    const load = async () => {
      let appearance = initialAppearance;
      let changed = false;
      const stopPreferences = await listen("system:changed", event => {
        const parsed = systemSnapshotSchema.safeParse(event.payload);
        if (!parsed.success) return;
        changed = true;
        appearance = parsed.data.preferences.terminal;
        void applyAppearance(appearance, true);
      });
      if (disposed) {
        stopPreferences();
        return;
      }
      unlistenPreferences = stopPreferences;
      try {
        const parsed = systemSnapshotSchema.safeParse(await invoke("get_system_preferences"));
        if (!changed && parsed.success) appearance = parsed.data.preferences.terminal;
      } catch {
        // Terminal availability is more important than a cosmetic preference read.
      }
      // Canvas measurement must happen after the selected system font is available.
      await applyAppearance(appearance, false);
      if (disposed) return;
      xterm.open(element);
      opened = true;
      observer.observe(element);
      resize();
      const stop = await listen("terminals:output", event => {
        const result = terminalOutputEventSchema.safeParse(event.payload);
        if (!result.success || result.data.conversationId !== conversationId || result.data.id !== terminal.id) return;
        if (!hydrated) {
          pendingOutput.push(result.data);
          return;
        }
        if (result.data.revision <= lastRevision) return;
        lastRevision = result.data.revision;
        xterm.write(result.data.data);
      });
      if (disposed) {
        stop();
        return;
      }
      unlistenOutput = stop;
      const result = terminalSnapshotSchema.parse(await invoke("read_chat_terminal", { conversationId, id: terminal.id }));
      if (disposed) return;
      xterm.reset();
      if (result.truncated) xterm.write("\r\n[Histórico local truncado]\r\n");
      xterm.write(result.output);
      lastRevision = result.revision;
      hydrated = true;
      for (const output of pendingOutput.sort((left, right) => left.revision - right.revision)) {
        if (output.revision <= lastRevision) continue;
        lastRevision = output.revision;
        xterm.write(output.data);
      }
      pendingOutput = [];
      resize();
      xterm.focus();
    };
    void load().catch(error => {
      if (!disposed) toast.error(libraryError(error, "Não foi possível abrir o terminal."));
    });
    return () => {
      disposed = true;
      observer.disconnect();
      input?.dispose();
      clearTimeout(inputTimer);
      unlistenOutput?.();
      unlistenPreferences?.();
      xterm.dispose();
    };
  }, [conversationId, terminal.id, terminal.status]);

  return <div ref={host} className="h-full min-h-0 min-w-0 w-full cursor-pointer bg-sidebar p-2 font-mono text-[13px]" aria-label={`Terminal ${terminal.title}`} role="application" />;
}
