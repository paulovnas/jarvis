import { useState } from "react";
import { Camera, Download } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { z } from "zod";
import { attachmentSchema } from "@/core/attachments";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Hint } from "@/components/ui/hint";
import { Skeleton } from "@/components/ui/skeleton";
import { StoredImage } from "./AttachmentPreview";
import type { ToolCallItem } from "./types";

function parse(output?: string) {
  try { return z.object({ attachment: attachmentSchema.extend({ kind: z.literal("image") }), url: z.string() }).parse(JSON.parse(output ?? "")); } catch { return null; }
}
export function BrowserCaptureCard({ tool }: { tool: ToolCallItem }) {
  const [open, setOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  if (tool.status === "pending" || tool.status === "running") return <div role="status" aria-label="Capturando página" className="relative my-2 aspect-video w-full max-w-lg"><Skeleton className="size-full" /><span className="absolute inset-0 flex items-center justify-center gap-2 text-xs text-muted-foreground"><Camera className="size-4" />Capturando página…</span></div>;
  const result = parse(tool.output);
  if (!result || tool.status === "error") return null;
  const save = async () => {
    setSaving(true);
    try { if (await invoke<boolean>("save_chat_image", { conversationId: result.attachment.conversationId, id: result.attachment.id })) toast.success("Captura salva"); }
    catch { toast.error("Não foi possível salvar a captura"); } finally { setSaving(false); }
  };
  return <section aria-label="Captura do navegador" className="my-2 w-full max-w-lg">
    <Button type="button" variant="ghost" aria-label="Ampliar captura do navegador" className="h-auto w-full cursor-pointer overflow-hidden rounded-md border border-border p-0" onClick={() => setOpen(true)}><StoredImage attachment={result.attachment} full /></Button>
    <Hint content={result.url}><p className="mt-1 truncate font-mono text-[10px] text-muted-foreground">{result.url}</p></Hint>
    <Dialog open={open} onOpenChange={setOpen}><DialogContent className="max-h-[90dvh] overflow-auto sm:max-w-4xl" aria-describedby={undefined}><DialogHeader><DialogTitle>Captura do navegador</DialogTitle></DialogHeader><StoredImage attachment={result.attachment} full /><Button type="button" variant="outline" className="cursor-pointer" disabled={saving} onClick={() => void save()}><Download className="size-4" />Salvar captura</Button></DialogContent></Dialog>
  </section>;
}
