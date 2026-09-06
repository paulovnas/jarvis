import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, Circle, CircleX, Send, ChevronRight } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from "@/components/ui/dialog";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Textarea } from "@/components/TextInput";
import { Label } from "@/components/ui/label";
import { libraryError } from "@/core/library";
import type { ValidationBatch, ValidationItem } from "@/core/workflow";

export function WorkflowValidation({ conversationId, batch, busy, onRefresh }: { conversationId: string; batch?: ValidationBatch | null; busy: boolean; onRefresh: () => void }) {
  const [selected, setSelected] = useState<string | null>(null);
  const [rejectOpen, setRejectOpen] = useState(false);
  const [reason, setReason] = useState("");
  const [pending, setPending] = useState(false);
  const [saved, setSaved] = useState<Record<string, Pick<ValidationItem, "decision" | "reason">>>({});
  const [sent, setSent] = useState(false);
  const lock = useRef(false);
  if (!batch) return <p className="text-xs text-muted-foreground">Aguardando itens de validação.</p>;
  const items = batch.items.map(item => ({ ...item, ...saved[item.id] }));
  const item = items.find(item => item.id === selected);
  const submitted = batch.submitted || sent;
  const disabled = busy || pending || submitted || batch.stale;
  const allReviewed = items.length > 0 && items.every(item => item.decision !== "pending");
  async function decide(decision: "approved" | "rejected") {
    if (!item || !batch || disabled || lock.current || decision === "rejected" && !reason.trim()) return;
    lock.current = true; setPending(true);
    try {
      await invoke("decide_workflow_validation", { conversationId, batchId: batch.id, itemId: item.id, decision, reason: decision === "rejected" ? reason.trim() : null });
      setSaved(current => ({ ...current, [item.id]: { decision, reason: decision === "rejected" ? reason.trim() : null } }));
      setSelected(null); setRejectOpen(false); onRefresh();
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível salvar a validação.")); }
    finally { lock.current = false; setPending(false); }
  }
  async function submit() {
    if (!batch || disabled || !allReviewed || lock.current) return;
    lock.current = true; setPending(true);
    try { await invoke("submit_workflow_validation", { conversationId, batchId: batch.id }); setSent(true); onRefresh(); toast.success("Resultados encaminhados ao Planejador."); }
    catch (cause) { toast.error(libraryError(cause, "Não foi possível encaminhar os resultados.")); }
    finally { lock.current = false; setPending(false); }
  }
  return <div className="space-y-2">
    {batch.stale && <Badge variant="outline" className="text-[10px] text-onedark-yellow">Aguardando nova rodada</Badge>}
    {submitted && !batch.stale && <Badge variant="outline" className="text-[10px] text-primary">Encaminhado ao Planejador</Badge>}
    <ul className="space-y-1">{items.map(item => <li key={item.id}><Button variant="ghost" onClick={() => { setSelected(item.id); setReason(item.reason ?? ""); setRejectOpen(false); }} aria-label={`${item.title}: ${item.decision === "approved" ? "aprovado" : item.decision === "rejected" ? "reprovado" : "pendente"}`} className={`h-auto w-full cursor-pointer justify-start gap-2 rounded-md border px-2.5 py-2 text-left text-xs whitespace-normal ${item.decision === "approved" ? "border-onedark-green/15 text-onedark-green" : item.decision === "rejected" ? "border-destructive/40 bg-destructive/5 text-destructive" : "border-border text-foreground"}`}>
      {item.decision === "approved" ? <Check aria-hidden="true" className="size-3.5 shrink-0" /> : item.decision === "rejected" ? <CircleX aria-hidden="true" className="size-3.5 shrink-0" /> : <Circle aria-hidden="true" className="size-3.5 shrink-0 text-muted-foreground" />}
      <span className={`min-w-0 flex-1 leading-5 ${item.decision === "approved" ? "line-through decoration-onedark-green/60" : ""}`}>{item.title}</span><ChevronRight aria-hidden="true" className="size-3 shrink-0 opacity-50" />
    </Button></li>)}</ul>
    {allReviewed && !submitted && !batch.stale && <Button size="sm" variant="outline" disabled={disabled} onClick={() => void submit()} className="mt-2 w-full cursor-pointer gap-2 text-xs"><Send aria-hidden="true" className="size-3.5" />Encaminhar resultado</Button>}
    <Dialog open={!!item} onOpenChange={value => { if (!value && !pending) { setSelected(null); setRejectOpen(false); } }}>
      {item && <DialogContent className="dark flex max-h-[85dvh] flex-col gap-0 overflow-hidden p-0 sm:max-w-lg">
        <DialogHeader className="border-b border-border bg-sidebar p-5 pr-12"><DialogDescription className="micro-label text-onedark-green">Validação manual</DialogDescription><DialogTitle className="text-base leading-6">{item.title}</DialogTitle></DialogHeader>
        <div className="min-h-0 space-y-5 overflow-y-auto p-5 text-sm"><ol className="list-decimal space-y-3 pl-5 marker:font-mono marker:text-muted-foreground">{item.steps.map((step, index) => <li key={index} className="whitespace-pre-wrap break-words pl-1 leading-6">{step}</li>)}</ol><div className="rounded-md border border-onedark-green/20 bg-onedark-green/5 p-3"><p className="micro-label mb-2 text-onedark-green">Resultado esperado</p><p className="whitespace-pre-wrap break-words text-xs leading-6">{item.expected}</p></div>{item.reason && <p className="whitespace-pre-wrap break-words text-xs leading-5 text-destructive">{item.reason}</p>}</div>
        {!submitted && !batch.stale && <DialogFooter className="border-t border-border bg-sidebar p-4">
          <Popover open={rejectOpen} onOpenChange={setRejectOpen}><PopoverTrigger render={<Button variant="outline" disabled={disabled} />} className="cursor-pointer text-destructive">Reprovar</PopoverTrigger><PopoverContent side="top" align="end" className="dark w-80 max-w-[85vw] gap-3 p-4"><Label htmlFor="validation-rejection">O que não funcionou?</Label><Textarea id="validation-rejection" autoFocus maxLength={4000} value={reason} onChange={event => setReason(event.target.value)} className="min-h-24 resize-y text-sm" /><Button variant="destructive" disabled={disabled || !reason.trim()} onClick={() => void decide("rejected")}>Confirmar reprovação</Button></PopoverContent></Popover>
          <Button disabled={disabled} onClick={() => void decide("approved")} className="cursor-pointer gap-2"><Check className="size-3.5" />Aprovar</Button>
        </DialogFooter>}
      </DialogContent>}
    </Dialog>
  </div>;
}
