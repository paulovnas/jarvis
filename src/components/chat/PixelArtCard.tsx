import { useContext, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, Copy, Download, Grid2X2 } from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import type { Attachment } from "@/core/attachments";
import { writeClipboardText } from "@/core/clipboard";
import { pixelArtResultSchema, readPixelArtResult, type PixelArtResult } from "@/core/pixel-art";
import { requestPixelArtSuite } from "@/hooks/use-pixel-art";
import { PixelArtResultsContext } from "@/hooks/use-pixel-art-results";
import { StoredImage } from "./AttachmentPreview";
import type { ToolCallItem } from "./types";

export function PixelArtCards({ tools }: { tools: ToolCallItem[] }) {
  const saved = useContext(PixelArtResultsContext);
  const jobs = new Map<string, PixelArtResult>();
  for (const tool of tools) {
    if (!tool.name.startsWith("pixel_art_")) continue;
    // History defers large tool outputs; durable receipts retain the call identity.
    const jobId = tool.args?._jarvisHistoryDetailsDeferred === true ? tool.args.__pixelArtJobId : undefined;
    const candidates = Object.values(saved).filter(job => jobId === undefined ? job.callId === tool.id : job.jobId === jobId);
    const result = readPixelArtResult(tool.output) ?? (candidates.length === 1 ? candidates[0] : undefined);
    if (!result) continue;
    if (["state", "configure", "inspect", "select"].includes(result.action)
      && !result.images.length && !result.artifacts.length
      && result.status !== "blocked" && result.status !== "failed") continue;
    const key = `${result.conversationId ?? ""}:${result.jobId}`;
    if ((jobs.get(key)?.revision ?? -1) <= result.revision) jobs.set(key, result);
  }
  return [...jobs.entries()].map(([key, result]) => <PixelArtCard key={key} result={result} />);
}

export function PixelArtCard({ result }: { result: PixelArtResult }) {
  const saved = useContext(PixelArtResultsContext)[result.jobId];
  const [live, setLive] = useState<PixelArtResult | null>(null);
  const [selected, setSelected] = useState<Attachment | null>(null);
  const [saving, setSaving] = useState(false);
  const [savingArtifact, setSavingArtifact] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    const subscription = listen<unknown>("pixel-art-progress", event => {
      if (!active) return;
      const parsed = pixelArtResultSchema.safeParse(event.payload);
      if (!parsed.success || parsed.data.jobId !== result.jobId || parsed.data.conversationId !== result.conversationId) return;
      setLive(previous => !previous || parsed.data.revision > previous.revision ? parsed.data : previous);
    });
    return () => { active = false; void subscription.then(unlisten => unlisten()).catch(() => undefined); };
  }, [result.jobId, result.conversationId]);
  const persisted = saved && saved.conversationId === result.conversationId && saved.revision > result.revision ? saved : result;
  const current = live && live.revision > persisted.revision ? live : persisted;
  const status = { running: "Em execução", completed: "Concluído", blocked: "Bloqueado", failed: "Falhou", cancelled: "Cancelado" }[current.status];
  const running = current.status === "running";
  const problem = current.status === "failed" || current.status === "blocked";
  const progress = current.progress;
  const copyPath = async (path: string) => {
    try { await writeClipboardText(path); toast.success("Caminho copiado"); }
    catch { toast.error("Não foi possível copiar o caminho"); }
  };
  const saveImage = async () => {
    if (!selected || saving) return;
    setSaving(true);
    try {
      if (await invoke<boolean>("save_chat_image", { conversationId: selected.conversationId, id: selected.id })) toast.success("Imagem salva");
    } catch { toast.error("Não foi possível salvar a imagem"); }
    finally { setSaving(false); }
  };
  const saveArtifact = async (path: string) => {
    if (!current.conversationId || savingArtifact) return;
    setSavingArtifact(path);
    try {
      if (await invoke<boolean>("save_pixel_art_artifact", { conversationId: current.conversationId, jobId: current.jobId, path })) toast.success("Arquivo salvo");
    } catch { toast.error("Não foi possível salvar o arquivo"); }
    finally { setSavingArtifact(null); }
  };
  return <section aria-label="Pixel art" className="my-2 min-w-0 space-y-3 rounded-lg border border-border bg-card p-3">
    <div className="flex items-center gap-2 text-xs">
      <Grid2X2 className="size-4 shrink-0 text-onedark-purple" aria-hidden="true" />
      <span className="min-w-0 flex-1 font-medium">Pixel art</span>
      {current.conversationId && <Button variant="ghost" size="sm" className="h-6 cursor-pointer px-2 text-xs" onClick={() => requestPixelArtSuite(current.conversationId!)}>Abrir suíte</Button>}
      <Badge variant="outline" className={problem ? "text-destructive" : "text-muted-foreground"}>{status}</Badge>
    </div>
    <p role={running ? "status" : undefined} className="text-xs text-muted-foreground">{current.phase}</p>
    {(running || progress) && <div className="space-y-1">
      <Progress aria-label="Progresso de pixel art" value={progress ? progress.completed / progress.total * 100 : null} />
      {progress && <p className="text-right font-mono text-[10px] text-muted-foreground">{progress.completed} / {progress.total} {progress.unit}</p>}
    </div>}
    {current.error && <p role="alert" className="whitespace-pre-wrap text-xs text-destructive">{current.error}</p>}
    {current.nextAction && <p className="whitespace-pre-wrap text-xs text-foreground">{current.nextAction}</p>}
    {current.images.length > 0 && <div className={`grid gap-2 ${current.images.length > 1 ? "grid-cols-2" : "grid-cols-1"}`}>
      {current.images.map(image => <Button key={image.id} variant="ghost" aria-label={`Ampliar ${image.name}`} className="h-auto w-full cursor-pointer overflow-hidden rounded-lg border border-border p-0 [image-rendering:pixelated]" onClick={() => setSelected(image)}>
        <StoredImage attachment={image} full />
      </Button>)}
    </div>}
    {current.artifacts.length > 0 && <div role="group" aria-label="Arquivos gerados" className="space-y-1">
      {current.artifacts.map(file => <div key={file.path}><div className="flex min-w-0 items-center gap-1"><Button variant="ghost" size="sm" className="h-auto min-w-0 flex-1 cursor-pointer justify-start gap-2 whitespace-normal px-1 text-left text-xs" title={file.path} aria-label={`Copiar caminho de ${file.name}`} onClick={() => void copyPath(file.path)}>
        <Copy className="size-3 shrink-0" aria-hidden="true" /><span className="min-w-0 break-all font-mono text-[10px]">{file.name}</span>
      </Button>{current.conversationId && <Button variant="ghost" size="icon-sm" className="shrink-0 cursor-pointer" disabled={savingArtifact !== null} aria-label={`Salvar ${file.name}`} onClick={() => void saveArtifact(file.path)}><Download className="size-3.5" /></Button>}</div>{file.previewError && <p className="px-1 text-xs text-muted-foreground">{file.previewError}</p>}</div>)}
    </div>}
    {(current.logs.length > 0 || current.logPath) && <Collapsible>
      <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="w-full cursor-pointer justify-between px-1 text-xs">
        Logs da geração<ChevronDown className="size-3" aria-hidden="true" />
      </CollapsibleTrigger>
      <CollapsibleContent className="space-y-2">
        <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted p-3 font-mono text-[10px]">{current.logs.join("\n")}</pre>
        {current.logPath && <Button variant="outline" size="sm" className="cursor-pointer text-xs" onClick={() => void copyPath(current.logPath!)}><Copy className="size-3" />Copiar caminho do log completo</Button>}
      </CollapsibleContent>
    </Collapsible>}
    <Dialog open={selected !== null} onOpenChange={open => { if (!open) setSelected(null); }}>
      <DialogContent className="max-h-[90dvh] overflow-y-auto sm:max-w-4xl" aria-describedby={undefined}>
        <DialogHeader><DialogTitle className="truncate pr-6 text-sm">{selected?.name}</DialogTitle></DialogHeader>
        {selected && <div className="[image-rendering:pixelated]"><StoredImage key={selected.id} attachment={selected} full showMetadata /></div>}
        <div className="flex justify-center"><Button variant="outline" className="cursor-pointer" disabled={saving} onClick={() => void saveImage()}><Download className="size-4" />Salvar imagem</Button></div>
      </DialogContent>
    </Dialog>
  </section>;
}
