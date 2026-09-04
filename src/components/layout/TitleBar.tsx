import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { JarvisLogo } from "@/components/JarvisLogo";

export function TitleBar() {
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    async function syncWindowState() {
      try {
        const appWindow = getCurrentWindow();
        setIsMaximized(await appWindow.isMaximized());
        unlisten = await appWindow.onResized(async () => {
          setIsMaximized(await appWindow.isMaximized());
        });
      } catch {
        // Ignored when running outside Tauri native runtime
      }
    }

    void syncWindowState();

    return () => {
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
      className="flex h-9 w-full shrink-0 select-none items-center justify-between border-b border-[#3e4451]/70 bg-[#21252b] px-3 text-xs font-medium text-muted-foreground transition-colors"
    >
      {/* Left: Branding & Context */}
      <div data-tauri-drag-region className="flex items-center gap-2 min-w-0">
        <JarvisLogo className="size-4 shrink-0" />
        <span className="font-semibold text-foreground tracking-wide text-[12px]">
          Jarvis
        </span>
        <span className="text-[#3e4451]">•</span>
        <span className="text-[11px] text-muted-foreground/80 truncate">
          Onboarding
        </span>
      </div>

      {/* Center: Draggable Spacer */}
      <div data-tauri-drag-region className="flex-1 h-full" />

      {/* Right: Window Controls */}
      <div className="flex items-center -mr-3 h-full">
        <button
          type="button"
          aria-label="Minimizar janela"
          onClick={handleMinimize}
          className="flex h-full w-11 items-center justify-center text-muted-foreground hover:bg-[#2c313a] hover:text-foreground transition-colors cursor-pointer"
        >
          <Minus className="size-3.5" />
        </button>

        <button
          type="button"
          aria-label={isMaximized ? "Restaurar janela" : "Maximizar janela"}
          onClick={handleToggleMaximize}
          className="flex h-full w-11 items-center justify-center text-muted-foreground hover:bg-[#2c313a] hover:text-foreground transition-colors cursor-pointer"
        >
          {isMaximized ? (
            <Copy className="size-3 rotate-180" />
          ) : (
            <Square className="size-3" />
          )}
        </button>

        <button
          type="button"
          aria-label="Fechar janela"
          onClick={handleClose}
          className="flex h-full w-11 items-center justify-center text-muted-foreground hover:bg-[#e06c75] hover:text-white transition-colors cursor-pointer"
        >
          <X className="size-3.5" />
        </button>
      </div>
    </header>
  );
}
