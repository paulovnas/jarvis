import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { JarvisLogo } from "@/components/JarvisLogo";
import { Button } from "@/components/ui/button";

export type TitleBarContext = "Iniciando" | "Onboarding" | "Início";

type TitleBarProps = {
  context?: TitleBarContext;
};

export function TitleBar({ context = "Onboarding" }: TitleBarProps) {
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
      onDoubleClick={handleToggleMaximize}
      className="flex h-9 w-full shrink-0 select-none items-center justify-between border-b border-border bg-sidebar px-3 text-xs font-medium text-muted-foreground shadow-[inset_0_1px_0_#ffffff08]"
    >
      {/* Left: Branding & Context */}
      <div data-tauri-drag-region className="flex items-center gap-2 min-w-0">
        <JarvisLogo className="size-4 shrink-0" />
        <span className="font-semibold uppercase text-foreground tracking-[.2em] text-[11px]">
          Jarvis
        </span>
        <span className="mx-1 h-3 border-l border-border" />
        <span className="font-mono text-[10px] text-muted-foreground truncate">
          {context}
        </span>
      </div>

      {/* Center: Draggable Spacer */}
      <div data-tauri-drag-region className="flex-1 h-full" />

      {/* Right: Window Controls */}
      <div className="flex items-center -mr-3 h-full">
        <Button variant="ghost"
          type="button"
          aria-label="Minimizar janela"
          onClick={handleMinimize}
          className="titlebar-control flex h-full w-10 items-center justify-center text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors cursor-pointer"
        >
          <Minus className="size-3.5" />
        </Button>

        <Button variant="ghost"
          type="button"
          aria-label={isMaximized ? "Restaurar janela" : "Maximizar janela"}
          onClick={handleToggleMaximize}
          className="titlebar-control flex h-full w-10 items-center justify-center text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors cursor-pointer"
        >
          {isMaximized ? (
            <Copy className="size-3 rotate-180" />
          ) : (
            <Square className="size-3" />
          )}
        </Button>

        <Button variant="ghost"
          type="button"
          aria-label="Fechar janela"
          onClick={handleClose}
          className="titlebar-control flex h-full w-10 items-center justify-center text-muted-foreground hover:bg-destructive hover:text-white transition-colors cursor-pointer"
        >
          <X className="size-3.5" />
        </Button>
      </div>
    </header>
  );
}
