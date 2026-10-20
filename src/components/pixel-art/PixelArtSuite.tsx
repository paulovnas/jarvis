import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { Grid2X2 } from "lucide-react";
import { browserOccluded } from "@/core/browser";
import { Skeleton } from "@/components/ui/skeleton";
import type { PixelArtController } from "@/hooks/use-pixel-art";

export function PixelArtSuite({ pixel }: { pixel: PixelArtController }) {
  const element = useRef<HTMLDivElement>(null);
  const [ready, setReady] = useState(false);
  useEffect(() => {
    const node = element.current;
    if (!node) return;
    let alive = true;
    let scheduled = false;
    let previous = "";
    let serial = Promise.resolve();
    const update = () => {
      scheduled = false;
      if (!alive) return;
      const rect = node.getBoundingClientRect();
      // The native parent owns window visibility; CEF may suspend this renderer while its slot is visible.
      const hidden = browserOccluded(document) || rect.width < 20 || rect.height < 20;
      const viewport = hidden ? null : { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
      const key = JSON.stringify(viewport);
      if (key === previous) return;
      previous = key;
      serial = serial.then(async () => {
        if (alive) {
          await invoke("set_pixel_art_viewport", { conversationId: pixel.conversationId, viewport });
          if (alive && viewport) setReady(true);
        }
      }).catch(cause => { if (alive) { previous = ""; toast.error(typeof cause === "string" ? cause : "Não foi possível exibir a suíte de pixel art.", { id: `pixel-viewport:${pixel.conversationId}` }); } });
    };
    const schedule = () => { if (!scheduled) { scheduled = true; queueMicrotask(update); } };
    const resize = new ResizeObserver(schedule);
    for (let parent: HTMLElement | null = node; parent; parent = parent.parentElement) resize.observe(parent);
    const mutations = new MutationObserver(schedule);
    mutations.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-open", "data-closed", "aria-hidden", "hidden"] });
    window.addEventListener("resize", schedule);
    document.addEventListener("visibilitychange", schedule);
    document.addEventListener("scroll", schedule, true);
    schedule();
    return () => {
      alive = false; resize.disconnect(); mutations.disconnect();
      window.removeEventListener("resize", schedule); document.removeEventListener("visibilitychange", schedule); document.removeEventListener("scroll", schedule, true);
      void serial.then(() => invoke("set_pixel_art_viewport", { conversationId: pixel.conversationId, viewport: null })).catch(() => undefined);
    };
  }, [pixel.conversationId]);
  return <section aria-label="Suíte de pixel art" className="flex h-full min-h-0 flex-col bg-background">
    <div className="flex min-h-9 shrink-0 items-center gap-2 border-b border-border px-3 text-xs text-muted-foreground">
      <Grid2X2 className="size-3.5 text-onedark-purple" aria-hidden="true" />
      <span>Pixel art</span><span className="ml-auto">Edições compartilhadas com o agente</span>
    </div>
    <div ref={element} className="min-h-0 flex-1">
      {!ready && <div role="status" aria-label="Carregando suíte de pixel art" className="grid h-full grid-cols-[1fr_2fr] gap-4 p-4">
        <Skeleton className="h-full" /><div className="flex flex-col gap-4"><Skeleton className="min-h-0 flex-1" /><Skeleton className="h-28" /></div>
      </div>}
    </div>
  </section>;
}
