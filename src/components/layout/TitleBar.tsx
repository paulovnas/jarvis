import { useEffect, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Maximize2, Minimize2, Minus, X } from "lucide-react";
import { JarvisLogo } from "@/components/JarvisLogo";
import { Button } from "@/components/ui/button";
import { Hint } from "@/components/ui/hint";
import { toast } from "sonner";

const resizeCursors = {
  North: "ns-resize", South: "ns-resize", West: "ew-resize", East: "ew-resize",
  NorthWest: "nwse-resize", SouthEast: "nwse-resize", NorthEast: "nesw-resize", SouthWest: "nesw-resize",
} as const;

function resizeDirection(event: MouseEvent): keyof typeof resizeCursors | null {
  const { clientX: x, clientY: y } = event;
  if (x < 0 || y < 0 || x > window.innerWidth || y > window.innerHeight) return null;
  const side = x < 6 ? "West" : x >= window.innerWidth - 6 ? "East" : null;
  if (y < 6) return side === "West" ? "NorthWest" : side === "East" ? "NorthEast" : "North";
  if (y >= window.innerHeight - 6) return side === "West" ? "SouthWest" : side === "East" ? "SouthEast" : "South";
  return side;
}

export function TitleBarSlot({ side, children }: { side: "left" | "right"; children: ReactNode }) {
  const target = document.getElementById(`titlebar-${side}-navigation`);
  return target ? createPortal(children, target) : children;
}

export function TitleBar() {
  const [isMaximized, setIsMaximized] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const isMac = /Mac/.test(navigator.platform);
  const isWindows = /Win/.test(navigator.platform);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let active = true;

    async function syncWindowState() {
      try {
        const appWindow = getCurrentWindow();
        const refresh = async () => {
          try {
            const [maximized, fullscreen] = await Promise.all([appWindow.isMaximized(), appWindow.isFullscreen()]);
            if (active) { setIsMaximized(maximized); setIsFullscreen(fullscreen); }
          } catch { /* The window may close while its resize event is pending. */ }
        };
        await refresh();
        unlisten = await appWindow.onResized(() => { void refresh(); });
        if (!active) unlisten();
      } catch {
        // Ignored when running outside Tauri native runtime
      }
    }

    void syncWindowState();

    return () => {
      active = false;
      if (unlisten) unlisten();
    };
  }, [isMac]);

  useEffect(() => {
    if (!isWindows || isMaximized || isFullscreen) return;
    let appWindow: ReturnType<typeof getCurrentWindow>;
    try { appWindow = getCurrentWindow(); } catch { return; }
    const root = document.documentElement;
    const clearCursor = () => {
      root.removeAttribute("data-native-window-resize");
      root.style.removeProperty("--native-window-resize-cursor");
    };
    const hover = (event: PointerEvent) => {
      const direction = event.buttons === 0 ? resizeDirection(event) : null;
      if (!direction) { clearCursor(); return; }
      root.setAttribute("data-native-window-resize", direction);
      root.style.setProperty("--native-window-resize-cursor", resizeCursors[direction]);
    };
    let consumedResize = false;
    const resize = (event: PointerEvent) => {
      consumedResize = false;
      if (event.defaultPrevented || event.button !== 0) return;
      const direction = resizeDirection(event);
      if (!direction) return;
      consumedResize = true;
      // Window capture precedes the panel library's document-level hit testing.
      event.preventDefault();
      event.stopImmediatePropagation();
      void appWindow.startResizeDragging(direction).catch(() => toast.error("Não foi possível redimensionar a janela."));
    };
    const blockEdgeClick = (event: MouseEvent) => {
      // Canceling pointerdown does not suppress Chromium's later click events.
      if (event.detail === 0 || event.button !== 0 || (!consumedResize && !resizeDirection(event))) return;
      event.preventDefault();
      event.stopImmediatePropagation();
    };
    window.addEventListener("pointerdown", resize, true);
    window.addEventListener("pointermove", hover, true);
    window.addEventListener("click", blockEdgeClick, true);
    window.addEventListener("dblclick", blockEdgeClick, true);
    window.addEventListener("blur", clearCursor);
    root.addEventListener("pointerleave", clearCursor);
    return () => {
      window.removeEventListener("pointerdown", resize, true);
      window.removeEventListener("pointermove", hover, true);
      window.removeEventListener("click", blockEdgeClick, true);
      window.removeEventListener("dblclick", blockEdgeClick, true);
      window.removeEventListener("blur", clearCursor);
      root.removeEventListener("pointerleave", clearCursor);
      clearCursor();
    };
  }, [isWindows, isMaximized, isFullscreen]);

  const handleMinimize = async () => {
    try {
      await getCurrentWindow().minimize();
    } catch {
      // Ignored outside Tauri
    }
  };

  const handleToggleMaximize = async () => {
    try {
      const appWindow = getCurrentWindow();
      await appWindow.toggleMaximize();
      setIsMaximized(await appWindow.isMaximized());
    } catch {
      setIsMaximized((prev) => !prev);
    }
  };

  const handleClose = async () => {
    try {
      await getCurrentWindow().close();
    } catch {
      // Ignored outside Tauri
    }
  };

  const handleGreen = async () => {
    if (!isMac) { await handleToggleMaximize(); return; }
    try {
      const appWindow = getCurrentWindow();
      const next = !(await appWindow.isFullscreen());
      await appWindow.setFullscreen(next);
      setIsFullscreen(next);
    } catch { /* Not available outside the native runtime. */ }
  };
  const expanded = isMac ? isFullscreen : isMaximized;
  const greenLabel = isMac ? (isFullscreen ? "Sair da tela cheia" : "Entrar em tela cheia") : (isMaximized ? "Restaurar janela" : "Maximizar janela");

  // macOS keeps its traffic-light cluster before the shared app navigation.
  // Windows (and Linux) follow the native convention: branding on the
  // left, square caption controls on the right with a destructive close hover.
  const controls = (
    <div aria-label="Controles da janela" className={isMac ? "group flex items-center" : "flex self-stretch"} onDoubleClick={event => event.stopPropagation()}>
      {isMac ? (
        <>
          <Hint content="Fechar"><Button variant="ghost" type="button" aria-label="Fechar janela" onClick={handleClose} className="traffic-light size-6 cursor-pointer rounded-full p-1.5 hover:bg-transparent">
            <span className="flex size-3 shrink-0 items-center justify-center rounded-full border border-black/10 bg-[#ff5f57]"><X className="size-2.5 text-black/65 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" /></span>
          </Button></Hint>
          <Hint content="Minimizar"><Button variant="ghost" type="button" aria-label="Minimizar janela" onClick={handleMinimize} className="traffic-light size-6 cursor-pointer rounded-full p-1.5 hover:bg-transparent">
            <span className="flex size-3 shrink-0 items-center justify-center rounded-full border border-black/10 bg-[#febc2e]"><Minus className="size-2.5 text-black/65 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" /></span>
          </Button></Hint>
          <Hint content={greenLabel}><Button variant="ghost" type="button" aria-label={greenLabel} onClick={handleGreen} className="traffic-light size-6 cursor-pointer rounded-full p-1.5 hover:bg-transparent">
            <span className="flex size-3 shrink-0 items-center justify-center rounded-full border border-black/10 bg-[#28c840]">{expanded ? <Minimize2 className="size-2 text-black/65 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" /> : <Maximize2 className="size-2 text-black/65 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" />}</span>
          </Button></Hint>
        </>
      ) : (
        <>
          <Hint content="Minimizar"><Button variant="ghost" type="button" aria-label="Minimizar janela" onClick={handleMinimize} className="h-full w-11 cursor-pointer rounded-none px-0">
            <Minus className="size-3.5" />
          </Button></Hint>
          <Hint content={greenLabel}><Button variant="ghost" type="button" aria-label={greenLabel} onClick={handleGreen} className="h-full w-11 cursor-pointer rounded-none px-0">
            {expanded ? <Minimize2 className="size-3.5" /> : <Maximize2 className="size-3.5" />}
          </Button></Hint>
          <Hint content="Fechar"><Button variant="ghost" type="button" aria-label="Fechar janela" onClick={handleClose} className="h-full w-11 cursor-pointer rounded-none px-0 hover:bg-destructive/15 hover:text-destructive">
            <X className="size-3.5" />
          </Button></Hint>
        </>
      )}
    </div>
  );

  return (
    <header
      aria-label="Barra superior"
      data-tauri-drag-region
      onDoubleClick={event => { if (!(event.target instanceof Element) || !event.target.closest("button")) void handleToggleMaximize(); }}
      className="native-titlebar relative flex h-10 w-full shrink-0 select-none items-center gap-2 bg-background pl-2 text-[13px] font-medium text-muted-foreground"
    >
      {isMac && !isFullscreen && controls}
      <div id="titlebar-left-navigation" className="flex shrink-0 items-center" />
      <div data-tauri-drag-region className="flex min-w-0 items-center">
        <JarvisLogo variant="horizontal" alt="Jarvis" className="pointer-events-none h-6 w-auto shrink-0" />
      </div>
      <div data-tauri-drag-region className="h-full flex-1" />
      <div id="titlebar-right-navigation" className="flex shrink-0 items-center" />
      {!isMac && controls}
    </header>
  );
}
