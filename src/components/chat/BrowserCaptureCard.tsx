import { useState } from "react";
import { Camera, Download } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { z } from "zod";
import { attachmentSchema } from "@/core/attachments";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Hint } from "@/components/ui/hint";
import { Skeleton } from "@/components/ui/skeleton";
import { StoredImage } from "./AttachmentPreview";
import type { ToolCallItem } from "./types";

function parse(output?: string) {
  try { return z.object({ attachment: attachmentSchema.extend({ kind: z.literal("image") }), url: z.string().optional(), observation: z.object({ window: z.object({ title: z.string() }) }).optional() }).parse(JSON.parse(output ?? "")); } catch { return null; }
}
function isUnavailableObservation(output?: string) {
  try { return z.object({ targetId: z.string().min(1), available: z.literal(false), code: z.literal("computer_target_unavailable"), phase: z.literal("observed"), inputSent: z.literal(false) }).safeParse(JSON.parse(output ?? "")).success; } catch { return false; }
}
export function BrowserCaptureCard({ tool }: { tool: ToolCallItem }) {
  const [open, setOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const native = tool.name.startsWith("computer_");
  const title = native ? "Captura do aplicativo" : "Captura do navegador";
  if (tool.status === "pending" || tool.status === "running") return native ? null : <div role="status" aria-label="Capturando página" className="relative my-2 aspect-video w-full max-w-lg"><Skeleton className="size-full" /><span className="absolute inset-0 flex items-center justify-center gap-2 text-xs text-muted-foreground"><Camera className="size-4" />Capturando página…</span></div>;
  if (tool.status === "completed" && tool.name === "computer_observe" && isUnavailableObservation(tool.output)) return <Card role="status" aria-label="Observação do aplicativo indisponível" className="my-2 w-full max-w-lg gap-0"><CardContent className="p-3 text-xs text-muted-foreground">A janela autorizada não está disponível. Nenhuma captura foi obtida.</CardContent></Card>;
  const result = parse(tool.output);
  if (!result || tool.status === "error" || !native && result.url === undefined) return null;
  const save = async () => {
    setSaving(true);
    try { if (await invoke<boolean>("save_chat_image", { conversationId: result.attachment.conversationId, id: result.attachment.id })) toast.success("Captura salva"); }
    catch { toast.error("Não foi possível salvar a captura"); } finally { setSaving(false); }
  };
  const caption = native ? result.observation?.window.title || "Janela autorizada" : result.url ?? "";
  return <section aria-label={title} className="my-2 w-full max-w-lg">
    <Button type="button" variant="ghost" aria-label={native ? "Ampliar captura do aplicativo" : "Ampliar captura do navegador"} className="h-auto w-full cursor-pointer overflow-hidden rounded-md border border-border p-0" onClick={() => setOpen(true)}><StoredImage attachment={result.attachment} full /></Button>
    <Hint content={caption} whenTruncated><p className="mt-1 truncate font-mono text-[10px] text-muted-foreground">{caption}</p></Hint>
    <Dialog open={open} onOpenChange={setOpen}><DialogContent className="max-h-[90dvh] overflow-auto sm:max-w-4xl" aria-describedby={undefined}><DialogHeader><DialogTitle>{title}</DialogTitle></DialogHeader><StoredImage attachment={result.attachment} full /><Button type="button" variant="outline" className="cursor-pointer" disabled={saving} onClick={() => void save()}><Download className="size-4" />Salvar captura</Button></DialogContent></Dialog>
  </section>;
}
