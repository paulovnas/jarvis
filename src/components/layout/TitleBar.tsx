import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Maximize2, Minimize2, Minus, X } from "lucide-react";
import { JarvisLogo } from "@/components/JarvisLogo";
import { Button } from "@/components/ui/button";

export function TitleBar() {
  const [isMaximized, setIsMaximized] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const isMac = /Mac/.test(navigator.platform);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let active = true;

    async function syncWindowState() {
      try {
        const appWindow = getCurrentWindow();
        const refresh = async () => {
          try {
            const [maximized, fullscreen] = await Promise.all([appWindow.isMaximized(), isMac ? appWindow.isFullscreen() : false]);
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

  if (isMac && isFullscreen) return null;

  // macOS keeps the traffic-light cluster on the left and the wordmark on the
  // right. Windows (and Linux) follow the native convention: branding on the
  // left, square caption controls on the right with a destructive close hover.
  const controls = (
    <div aria-label="Controles da janela" className={isMac ? "group flex items-center gap-0.5" : "flex self-stretch"} onDoubleClick={event => event.stopPropagation()}>
      {isMac ? (
        <>
          <Button variant="ghost" type="button" aria-label="Fechar janela" title="Fechar" onClick={handleClose} className="traffic-light size-6 cursor-pointer rounded-full p-1.5 hover:bg-transparent">
            <span className="flex size-3 shrink-0 items-center justify-center rounded-full border border-black/10 bg-[#ff5f57]"><X className="size-2.5 text-black/65 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" /></span>
          </Button>
          <Button variant="ghost" type="button" aria-label="Minimizar janela" title="Minimizar" onClick={handleMinimize} className="traffic-light size-6 cursor-pointer rounded-full p-1.5 hover:bg-transparent">
            <span className="flex size-3 shrink-0 items-center justify-center rounded-full border border-black/10 bg-[#febc2e]"><Minus className="size-2.5 text-black/65 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" /></span>
          </Button>
          <Button variant="ghost" type="button" aria-label={greenLabel} title={greenLabel} onClick={handleGreen} className="traffic-light size-6 cursor-pointer rounded-full p-1.5 hover:bg-transparent">
            <span className="flex size-3 shrink-0 items-center justify-center rounded-full border border-black/10 bg-[#28c840]">{expanded ? <Minimize2 className="size-2 text-black/65 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" /> : <Maximize2 className="size-2 text-black/65 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" />}</span>
          </Button>
        </>
      ) : (
        <>
          <Button variant="ghost" type="button" aria-label="Minimizar janela" title="Minimizar" onClick={handleMinimize} className="h-full w-11 cursor-pointer rounded-none px-0">
            <Minus className="size-3.5" />
          </Button>
          <Button variant="ghost" type="button" aria-label={greenLabel} title={greenLabel} onClick={handleGreen} className="h-full w-11 cursor-pointer rounded-none px-0">
            {expanded ? <Minimize2 className="size-3.5" /> : <Maximize2 className="size-3.5" />}
          </Button>
          <Button variant="ghost" type="button" aria-label="Fechar janela" title="Fechar" onClick={handleClose} className="h-full w-11 cursor-pointer rounded-none px-0 hover:bg-destructive/15 hover:text-destructive">
            <X className="size-3.5" />
          </Button>
        </>
      )}
    </div>
  );

  return (
    <header
      data-tauri-drag-region
      onDoubleClick={event => { if (!(event.target instanceof Element) || !event.target.closest("button")) void handleToggleMaximize(); }}
      className="flex h-9 w-full shrink-0 select-none items-center border-b border-border bg-sidebar px-3 text-xs font-medium text-muted-foreground shadow-[inset_0_1px_0_#ffffff08]"
    >
      {isMac ? (
        <>
          {controls}
          <div data-tauri-drag-region className="h-full flex-1" />
          <div data-tauri-drag-region className="flex items-center gap-2 min-w-0">
            <JarvisLogo variant="horizontal" alt="Jarvis" className="pointer-events-none h-7 w-auto shrink-0" />
          </div>
        </>
      ) : (
        <>
          <div data-tauri-drag-region className="flex items-center gap-2 min-w-0">
            <JarvisLogo variant="horizontal" alt="Jarvis" className="pointer-events-none h-7 w-auto shrink-0" />
          </div>
          <div data-tauri-drag-region className="h-full flex-1" />
          {controls}
        </>
      )}
    </header>
  );
}
