import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Download, ExternalLink, Film } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { fileName, type VideoPreview } from "@/core/project-files";

export function VideoSkeleton() {
  return <div role="status" aria-label="Carregando vídeo" className="mx-auto w-full max-w-5xl space-y-3 p-5"><Skeleton className="aspect-video w-full rounded-md" /><div className="flex justify-end gap-2"><Skeleton className="h-8 w-28" /><Skeleton className="h-8 w-36" /></div></div>;
}

export function VideoViewer({ projectId, file }: { projectId: string; file: VideoPreview }) {
  const [loaded, setLoaded] = useState(false);
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState<"save" | "open" | null>(null);
  async function action(kind: "save" | "open") {
    if (busy) return;
    setBusy(kind);
    try {
      const result = await invoke<boolean | void>(kind === "save" ? "save_project_video" : "open_project_video", { projectId, path: file.path });
      if (kind === "save" && result) toast.success("Vídeo salvo");
    } catch { toast.error(kind === "save" ? "Não foi possível salvar o vídeo" : "Não foi possível abrir o vídeo no aplicativo padrão"); }
    finally { setBusy(null); }
  }
  return <section aria-label="Prévia do vídeo" className="mx-auto flex h-full w-full max-w-5xl flex-col gap-3 overflow-auto p-5">
    {failed ? <Alert><Film aria-hidden="true" /><AlertTitle>Não foi possível reproduzir este vídeo</AlertTitle><AlertDescription>O formato ou codec pode não ser compatível com o Jarvis. Salve o vídeo ou abra-o no aplicativo padrão.</AlertDescription></Alert> : <div className="relative min-h-0 flex-1 overflow-hidden rounded-md border border-border bg-sidebar">
      {!loaded && <Skeleton role="status" aria-label="Carregando vídeo" className="pointer-events-none absolute inset-0 size-full" />}
      <video aria-label={`Vídeo ${fileName(file.path)}`} src={file.url} controls playsInline preload="metadata" onLoadedMetadata={() => setLoaded(true)} onError={() => setFailed(true)} className="h-full min-h-48 w-full cursor-pointer object-contain">Seu navegador não suporta a reprodução de vídeo.</video>
    </div>}
    <div className="flex shrink-0 flex-wrap justify-end gap-2">
      <Button type="button" variant="outline" className="cursor-pointer" disabled={busy !== null} onClick={() => void action("save")}><Download data-icon="inline-start" />{busy === "save" ? "Salvando…" : "Salvar vídeo"}</Button>
      <Button type="button" variant="outline" className="cursor-pointer" disabled={busy !== null} onClick={() => void action("open")}><ExternalLink data-icon="inline-start" />Abrir no aplicativo padrão</Button>
    </div>
  </section>;
}
