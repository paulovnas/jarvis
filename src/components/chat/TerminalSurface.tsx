import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { toast } from "sonner";
import { libraryError } from "@/core/library";
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
    // Canvas font measurement cannot resolve CSS custom properties.
    const fontFamily = getComputedStyle(element).fontFamily || "monospace";
    const fontSize = 13;
    const xterm = new Terminal({
      cursorBlink: terminal.status === "running",
      cursorStyle: "bar",
      fontFamily,
      fontSize,
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
      // Measure the bundled font only after it is available, including on first open.
      await document.fonts?.load(`${fontSize}px ${fontFamily}`).catch(() => undefined);
      if (disposed) return;
      xterm.open(element);
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
      xterm.dispose();
    };
  }, [conversationId, terminal.id, terminal.status]);

  return <div ref={host} className="h-full min-h-0 min-w-0 w-full cursor-pointer bg-sidebar p-2 font-mono text-[13px]" aria-label={`Terminal ${terminal.title}`} role="application" />;
}
