import { useState } from "react";
import { Download, ImagePlus } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { readGeneratedImages } from "@/core/image-generation";
import type { Attachment } from "@/core/attachments";
import { StoredImage } from "./AttachmentPreview";
import type { ToolCallItem } from "./types";

export function GeneratedImageCard({ tool }: { tool: ToolCallItem }) {
  const [selected, setSelected] = useState<Attachment | null>(null);
  const [saving, setSaving] = useState(false);
  const pending = tool.status === "pending" || tool.status === "running";
  const result = readGeneratedImages(tool.output);
  async function save() {
    if (!selected || saving) return;
    setSaving(true);
    try { if (await invoke<boolean>("save_chat_image", { conversationId: selected.conversationId, id: selected.id })) toast.success("Imagem salva"); }
    catch { toast.error("Não foi possível salvar a imagem"); }
    finally { setSaving(false); }
  }
  return <section className="my-2 min-w-0 max-w-lg" aria-label="Geração de imagem">
    {pending ? <div role="status" aria-label="Gerando imagem" className="relative aspect-[4/3] overflow-hidden rounded-lg border border-white/10"><Skeleton className="absolute inset-0 size-full" /><span className="absolute inset-0 flex items-center justify-center gap-2 text-xs text-muted-foreground"><ImagePlus className="size-4" />Gerando imagem…</span></div> : result ? <>
      <div className={`grid gap-2 ${result.images.length > 1 ? "grid-cols-2" : "grid-cols-1"}`}>{result.images.map(attachment => <Button key={attachment.id} variant="ghost" className="h-auto w-full cursor-pointer overflow-hidden rounded-lg border border-white/10 p-0" aria-label={`Ampliar ${attachment.name}`} onClick={() => setSelected(attachment)}><StoredImage attachment={attachment} full /></Button>)}</div>
      <p className="mt-2 truncate font-mono text-[10px] text-muted-foreground">Gemini 3.1 Flash Image</p>
    </> : <p role="alert" className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-xs text-destructive">{tool.error || (tool.status === "error" ? tool.output : null) || "Imagem indisponível"}</p>}
    <Dialog open={selected !== null} onOpenChange={open => { if (!open) setSelected(null); }}>
      <DialogContent className="max-h-[90dvh] overflow-y-auto sm:max-w-4xl" aria-describedby={undefined}>
        <DialogHeader><DialogTitle className="truncate pr-6 text-sm">{selected?.name}</DialogTitle></DialogHeader>
        {selected && <StoredImage key={selected.id} attachment={selected} full showMetadata />}
        <div className="flex justify-center"><Button variant="outline" className="cursor-pointer" disabled={saving} onClick={() => void save()}><Download data-icon="inline-start" />Salvar imagem</Button></div>
      </DialogContent>
    </Dialog>
  </section>;
}
