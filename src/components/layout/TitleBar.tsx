import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Maximize2, Minimize2, Minus, X } from "lucide-react";
import { JarvisLogo } from "@/components/JarvisLogo";
import { Button } from "@/components/ui/button";

export function TitleBar() {
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let active = true;

    async function syncWindowState() {
      try {
        const appWindow = getCurrentWindow();
        const maximized = await appWindow.isMaximized();
        if (active) setIsMaximized(maximized);
        unlisten = await appWindow.onResized(async () => {
          const next = await appWindow.isMaximized();
          if (active) setIsMaximized(next);
        });
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
  }, []);

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

  return (
    <header
      data-tauri-drag-region
      onDoubleClick={event => { if (!(event.target instanceof Element) || !event.target.closest("button")) void handleToggleMaximize(); }}
      className="flex h-9 w-full shrink-0 select-none items-center justify-between border-b border-border bg-sidebar px-3 text-xs font-medium text-muted-foreground shadow-[inset_0_1px_0_#ffffff08]"
    >
      <div aria-label="Controles da janela" className="group flex items-center gap-0.5" onDoubleClick={event => event.stopPropagation()}>
        <Button variant="ghost" type="button" aria-label="Fechar janela" title="Fechar" onClick={handleClose} className="traffic-light size-6 cursor-pointer rounded-full p-1.5 hover:bg-transparent">
          <span className="flex size-3 shrink-0 items-center justify-center rounded-full border border-black/10 bg-[#ff5f57]"><X className="size-2.5 text-black/65 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" /></span>
        </Button>
        <Button variant="ghost" type="button" aria-label="Minimizar janela" title="Minimizar" onClick={handleMinimize} className="traffic-light size-6 cursor-pointer rounded-full p-1.5 hover:bg-transparent">
          <span className="flex size-3 shrink-0 items-center justify-center rounded-full border border-black/10 bg-[#febc2e]"><Minus className="size-2.5 text-black/65 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" /></span>
        </Button>
        <Button variant="ghost" type="button" aria-label={isMaximized ? "Restaurar janela" : "Maximizar janela"} title={isMaximized ? "Restaurar" : "Maximizar"} onClick={handleToggleMaximize} className="traffic-light size-6 cursor-pointer rounded-full p-1.5 hover:bg-transparent">
          <span className="flex size-3 shrink-0 items-center justify-center rounded-full border border-black/10 bg-[#28c840]">{isMaximized ? <Minimize2 className="size-2 text-black/65 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" /> : <Maximize2 className="size-2 text-black/65 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" />}</span>
        </Button>
      </div>
      <div data-tauri-drag-region className="h-full flex-1" />
      <div data-tauri-drag-region className="flex items-center gap-2 min-w-0">
        <JarvisLogo className="size-4 shrink-0" />
        <span className="font-semibold uppercase text-foreground tracking-[.2em] text-[11px]">
          Jarvis
        </span>
      </div>
    </header>
  );
}
