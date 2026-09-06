import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileText, ImageOff, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import type { Attachment } from "@/core/attachments";

function StoredImage({ attachment, full = false }: { attachment: Attachment; full?: boolean }) {
  const [source, setSource] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const [loaded, setLoaded] = useState(false);
  useEffect(() => {
    let active = true;
    void invoke<string>("get_chat_attachment_image", { conversationId: attachment.conversationId, id: attachment.id, full }).then(value => { if (active) setSource(value); }).catch(() => { if (active) setFailed(true); });
    return () => { active = false; };
  }, [attachment.conversationId, attachment.id, full]);
  return <span className={full ? "relative block w-full" : "relative block size-full"}>
    {failed ? <ImageOff aria-label="Imagem indisponível" className="m-auto size-5 text-muted-foreground" /> : <>
      {!loaded && <Skeleton role="status" aria-label="Carregando imagem" className={full ? "h-80 w-full" : "absolute inset-0 size-full"} />}
      {source && <img src={source} alt={attachment.name} onLoad={() => setLoaded(true)} onError={() => setFailed(true)} className={`${full ? "max-h-[70vh] w-full object-contain" : "size-full object-cover"} ${loaded ? "" : "absolute inset-0 opacity-0"}`} />}
    </>}
  </span>;
}
export function AttachmentPreview({ attachment, onRemove, disabled }: { attachment: Attachment; onRemove?: () => void; disabled?: boolean }) {
  const [open, setOpen] = useState(false);
  return <span className={`relative mr-2 mb-2 inline-flex min-w-0 max-w-full align-top ${attachment.kind === "image" ? "w-20" : "w-52"}`}>
    {attachment.kind === "image" ? <Button type="button" variant="outline" className="size-20 cursor-pointer overflow-hidden rounded-lg p-0" aria-label={`Ampliar ${attachment.name}`} onClick={() => setOpen(true)}><StoredImage key={attachment.id} attachment={attachment} /></Button> : <span className="inline-flex h-16 w-full min-w-0 items-center gap-2 rounded-lg border border-border bg-secondary/40 px-3 pr-7"><FileText className="size-5 shrink-0 text-primary" /><span className="min-w-0 flex-1"><span className="block truncate text-xs" title={attachment.name}>{attachment.name}</span><span className="block font-mono text-[10px] text-muted-foreground">{Math.max(1, Math.round(attachment.size / 1024))} KB</span></span></span>}
    {onRemove && <Button type="button" variant="secondary" size="icon" className="absolute -right-1 -top-1 size-5 cursor-pointer rounded-full border border-border shadow" aria-label={`Remover anexo ${attachment.name}`} disabled={disabled} onClick={onRemove}><X className="size-3" /></Button>}
    <Dialog open={open} onOpenChange={setOpen}><DialogContent className="sm:max-w-4xl" aria-describedby={undefined}><DialogHeader><DialogTitle className="truncate pr-6 text-sm">{attachment.name}</DialogTitle></DialogHeader>{open && <StoredImage attachment={attachment} full />}</DialogContent></Dialog>
  </span>;
}
