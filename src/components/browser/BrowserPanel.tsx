import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowLeft, ArrowRight, Camera, Globe, RefreshCw, SquareTerminal } from "lucide-react";
import { toast } from "sonner";
import { z } from "zod";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/TextInput";
import { Badge } from "@/components/ui/badge";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { browserAddress, browserConsoleSchema, browserOccluded, type BrowserLog, type BrowserTab } from "@/core/browser";
import { browserError as libraryError } from "@/core/browser";
import type { BrowserController } from "@/hooks/use-browser";

function NativeViewport({ browser, tab }: { browser: BrowserController; tab: BrowserTab }) {
  const element = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const node = element.current;
    if (!node) return;
    let alive = true;
    let scheduled = 0;
    let previous = "";
    let serial = Promise.resolve();
    const update = () => {
      scheduled = 0;
      if (!alive) return;
      const rect = node.getBoundingClientRect();
      const hidden = document.hidden || browserOccluded(document) || rect.width < 20 || rect.height < 20;
      const viewport = hidden ? null : { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
      const key = JSON.stringify(viewport);
      if (key === previous) return;
      previous = key;
      serial = serial.then(async () => {
        if (!alive) return;
        await invoke("set_browser_viewport", { conversationId: browser.conversationId, id: tab.id, viewport });
      }).catch(cause => { if (alive) { previous = ""; toast.error(libraryError(cause), { id: `browser-viewport:${tab.id}` }); } });
    };
    const schedule = () => { if (!scheduled) scheduled = requestAnimationFrame(update); };
    const resize = new ResizeObserver(schedule);
    // Observe ancestors too: resizing the inspector can move the viewport without
    // changing its own dimensions. Native children do not participate in DOM layout.
    for (let parent: HTMLElement | null = node; parent; parent = parent.parentElement) resize.observe(parent);
    const mutations = new MutationObserver(schedule);
    mutations.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-open", "data-closed", "aria-hidden", "hidden"] });
    window.addEventListener("resize", schedule);
    document.addEventListener("visibilitychange", schedule);
    document.addEventListener("scroll", schedule, true);
    schedule();
    return () => {
      alive = false; cancelAnimationFrame(scheduled); resize.disconnect(); mutations.disconnect();
      window.removeEventListener("resize", schedule); document.removeEventListener("visibilitychange", schedule); document.removeEventListener("scroll", schedule, true);
      void serial.then(() => invoke("set_browser_viewport", { conversationId: browser.conversationId, id: tab.id, viewport: null })).catch(() => {});
    };
  }, [browser.conversationId, tab.id, tab.url]);
  return <div ref={element} aria-label="Página do navegador" className="relative min-h-0 flex-1 overflow-hidden bg-background">
    <div className="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground"><Globe className="size-8" /><p className="text-sm">{tab.url === "about:blank" ? "Digite um endereço para começar a navegar." : "Conteúdo do navegador"}</p>{tab.loading && <div role="status" aria-label="Carregando página" className="w-56 space-y-2"><Skeleton className="h-3 w-full" /><Skeleton className="h-3 w-3/4" /></div>}</div>
  </div>;
}

export function BrowserPanel({ browser, tab }: { browser: BrowserController; tab: BrowserTab }) {
  const [draft, setDraft] = useState({ url: tab.url, value: tab.url === "about:blank" ? "" : tab.url });
  const address = draft.url === tab.url ? draft.value : tab.url === "about:blank" ? "" : tab.url;
  const [consoleOpen, setConsoleOpen] = useState(false);
  const [logs, setLogs] = useState<BrowserLog[]>([]);
  const [capture, setCapture] = useState<string | null>(null);
  const [capturing, setCapturing] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const readConsole = async () => {
    const value = await browser.command({ action: "console", id: tab.id });
    if (value !== undefined) {
      const parsed = browserConsoleSchema.safeParse(value);
      if (parsed.success) setLogs(parsed.data.logs);
      else toast.error("Não foi possível ler os logs da página.");
    }
  };
  const screenshot = async () => {
    setCapturing(true);
    try {
      const value = await browser.command({ action: "screenshot", id: tab.id });
      if (value === undefined) return;
      const { attachment } = z.object({ attachment: z.object({ id: z.string() }) }).parse(value);
      setCapture(await invoke<string>("get_chat_attachment_image", { conversationId: browser.conversationId, id: attachment.id, full: true }));
      toast.success("Captura salva nos anexos desta conversa.");
    } catch (cause) { toast.error(libraryError(cause)); } finally { setCapturing(false); }
  };
  return <div className="flex h-full min-h-0 flex-col">
    <form aria-label="Navegação" className="flex min-h-11 shrink-0 items-center gap-1 border-b border-border bg-card px-2" onSubmit={event => { event.preventDefault(); try { void browser.command({ action: "navigate", id: tab.id, url: browserAddress(address) }); } catch (cause) { toast.error(libraryError(cause)); } }}>
      <Button type="button" variant="ghost" size="icon" title="Voltar" aria-label="Voltar página" className="size-7 cursor-pointer" onClick={() => void browser.command({ action: "back", id: tab.id })}><ArrowLeft className="size-3.5" /></Button>
      <Button type="button" variant="ghost" size="icon" title="Avançar" aria-label="Avançar página" className="size-7 cursor-pointer" onClick={() => void browser.command({ action: "forward", id: tab.id })}><ArrowRight className="size-3.5" /></Button>
      <Button type="button" variant="ghost" size="icon" title="Recarregar" aria-label="Recarregar página" className="size-7 cursor-pointer" onClick={() => void browser.command({ action: "reload", id: tab.id })}><RefreshCw className="size-3.5" /></Button>
      <Input ref={input} aria-label="Endereço do navegador" placeholder="URL ou localhost:3000" autoFocus={tab.url === "about:blank"} value={address} onChange={event => setDraft({ url: tab.url, value: event.target.value })} onFocus={event => event.target.select()} className="mx-1 h-7 min-w-0 flex-1 font-mono text-xs" />
      <Button type="submit" size="sm" variant="secondary" className="h-7 cursor-pointer px-2 text-xs">Ir</Button>
      <Button type="button" variant={consoleOpen ? "secondary" : "ghost"} size="icon" title="Console" aria-label="Mostrar console" aria-pressed={consoleOpen} className="size-7 cursor-pointer" onClick={() => { setConsoleOpen(!consoleOpen); if (!consoleOpen) void readConsole(); }}><SquareTerminal className="size-3.5" /></Button>
      <Button type="button" variant="ghost" size="icon" title="Capturar página" aria-label="Capturar página" disabled={capturing} className="size-7 cursor-pointer" onClick={() => void screenshot()}><Camera className="size-3.5" /></Button>
    </form>
    <NativeViewport browser={browser} tab={tab} />
    {consoleOpen && <section aria-label="Console da página" className="flex h-44 min-h-0 shrink-0 flex-col border-t border-border bg-sidebar">
      <div className="flex h-8 shrink-0 items-center gap-2 border-b border-border px-3"><span className="text-xs font-medium">Console</span><Badge variant="secondary" className="font-mono text-[10px]">{logs.length}</Badge><span className="flex-1" /><Button type="button" variant="ghost" size="sm" className="h-6 cursor-pointer text-xs" onClick={() => void readConsole()}>Atualizar logs</Button></div>
      <div className="min-h-0 flex-1 overflow-auto px-3 py-2 font-mono text-[11px]">{logs.length ? logs.map((log, index) => <p key={`${log.time}:${index}`} className={`whitespace-pre-wrap break-all border-b border-border/50 py-1 ${log.level === "error" ? "text-destructive" : log.level === "warn" ? "text-onedark-yellow" : "text-muted-foreground"}`}><span className="mr-2 uppercase opacity-70">{log.level}</span>{log.text}</p>) : <p className="text-muted-foreground">Nenhum log registrado nesta página.</p>}</div>
    </section>}
    <div className="flex h-6 shrink-0 items-center justify-between gap-3 border-t border-border px-3 text-[10px] text-muted-foreground"><span className="truncate font-mono">{tab.url === "about:blank" ? "Nova aba" : tab.url}</span><span className="shrink-0">{tab.loading ? "Carregando…" : "Navegador"}</span></div>
    <Dialog open={capture !== null} onOpenChange={open => { if (!open) setCapture(null); }}><DialogContent className="flex max-h-[90vh] flex-col sm:max-w-5xl"><DialogHeader><DialogTitle>Captura do navegador</DialogTitle><DialogDescription>Imagem da área visível, salva nos anexos da conversa.</DialogDescription></DialogHeader>{capture && <img src={capture} alt="Captura da página aberta no navegador" className="min-h-0 w-full flex-1 object-contain" />}</DialogContent></Dialog>
  </div>;
}
