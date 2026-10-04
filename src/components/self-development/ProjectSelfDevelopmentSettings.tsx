import { useState } from "react";
import { Bug, Copy, RefreshCw, ShieldCheck, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { CardsSkeleton } from "@/components/layout/LoadingSkeletons";
import { writeClipboardText } from "@/core/clipboard";
import type { SelfDevelopmentController, SelfDevelopmentIncident } from "@/hooks/use-self-development";

const STATUS_LABELS: Record<string, string> = {
  idle: "Em espera", running: "Em execução", completed: "Concluída", failed: "Falhou", cancelled: "Cancelada", interrupted: "Interrompida", error: "Erro",
};
function sourceStatus(status: string) { return STATUS_LABELS[status] ?? "Registrada"; }

export function ProjectSelfDevelopmentSettings({ controller }: { controller: SelfDevelopmentController }) {
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [description, setDescription] = useState("");
  const [revoking, setRevoking] = useState<SelfDevelopmentIncident | null>(null);
  const [disabling, setDisabling] = useState(false);
  const [copying, setCopying] = useState<string | null>(null);
  if (!controller.status?.eligible) return null;
  const enabled = controller.status.enabled;
  const disabled = controller.loading || controller.pending;
  const selected = controller.sources.find(source => source.id === conversationId);

  const clearSelection = () => { setConversationId(null); setDescription(""); };
  const toggle = async (value: boolean) => {
    if (await controller.setEnabled(value)) {
      clearSelection(); setDisabling(false); setRevoking(null);
      toast.success(value ? "Ambiente de desenvolvimento ativado" : "Ambiente desativado e incidentes revogados");
    }
  };
  const capture = async () => {
    if (!selected) return;
    if (await controller.capture(selected.id, description)) { clearSelection(); toast.success("Incidente preparado para investigação"); }
  };
  const copy = async (incident: SelfDevelopmentIncident) => {
    if (!enabled || copying || !controller.incidents.some(item => item.id === incident.id)) return;
    setCopying(incident.id);
    try {
      await writeClipboardText(incident.reference);
      toast.success("Referência copiada. Cole no chat deste projeto Jarvis para iniciar a investigação.");
    } catch { toast.error("Não foi possível copiar a referência."); }
    finally { setCopying(null); }
  };

  return <div className="space-y-5">
    <Card>
      <CardHeader className="border-b border-border">
        <CardTitle className="flex items-center gap-2"><ShieldCheck aria-hidden="true" className="size-4 text-onedark-cyan" />Ambiente de desenvolvimento do Jarvis</CardTitle>
        <CardDescription>Autorize este projeto a investigar o próprio Jarvis usando dados desta instalação e deste dispositivo.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4 pt-5">
        <div className="flex items-center justify-between gap-6">
          <div className="space-y-1"><Label htmlFor="self-development-enabled">Ambiente de desenvolvimento do Jarvis</Label><p className="max-w-2xl text-xs leading-relaxed text-muted-foreground">A ativação não inicia uma conversa. Você escolhe e compartilha cada incidente. Desativar remove os incidentes e revoga o acesso dos agentes.</p></div>
          <Switch id="self-development-enabled" className="cursor-pointer" checked={enabled} disabled={disabled} onCheckedChange={value => { if (value) void toggle(true); else setDisabling(true); }} />
        </div>
        {controller.error && <Alert variant="destructive"><AlertDescription>{controller.error}</AlertDescription></Alert>}
        <div className="flex items-center justify-between gap-3"><p className="text-xs text-muted-foreground">Diagnósticos locais. Projetos comuns não recebem esta capacidade.</p><Button variant="ghost" size="sm" className="cursor-pointer" disabled={disabled} onClick={() => { clearSelection(); void controller.refresh(); }}><RefreshCw className="size-3.5" />Atualizar autorização</Button></div>
      </CardContent>
    </Card>

    {enabled && <Card>
      <CardHeader className="border-b border-border">
        <CardTitle className="flex items-center gap-2"><Bug aria-hidden="true" className="size-4 text-primary" />Investigar com o Jarvis</CardTitle>
        <CardDescription>Selecione uma única conversa da biblioteca deste dispositivo. Apenas o incidente preparado fica disponível aos agentes deste projeto.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-5 pt-5">
        <Alert><ShieldCheck aria-hidden="true" /><AlertDescription>O resumo permitido inclui versão, modelo, erros e sequência de ferramentas. Credenciais, texto bruto das mensagens e logs globais ficam fora do incidente.</AlertDescription></Alert>
        {controller.loading ? <CardsSkeleton label="Carregando conversas e incidentes deste dispositivo" /> : <>
          <div className="space-y-2"><Label htmlFor="self-development-source">Conversa de origem</Label><Select value={selected?.id ?? null} onValueChange={value => setConversationId(value)} disabled={disabled}>
            <SelectTrigger id="self-development-source" className="w-full cursor-pointer"><SelectValue placeholder="Escolha uma conversa">{selected ? `${selected.title} · ${selected.projectName}` : undefined}</SelectValue></SelectTrigger>
            <SelectContent>{controller.sources.map(source => <SelectItem key={source.id} value={source.id} className="cursor-pointer"><span className="truncate">{source.title} · {source.projectName} · {sourceStatus(source.status)}</span></SelectItem>)}</SelectContent>
          </Select><p className="text-[11px] text-muted-foreground">{controller.sources.length ? "Nenhuma conversa é selecionada ou capturada automaticamente." : "Nenhuma conversa está disponível nesta instalação."}</p></div>
          <div className="space-y-2"><Label htmlFor="self-development-description">O que aconteceu? (opcional)</Label><Textarea id="self-development-description" className="min-h-20" value={description} disabled={disabled} maxLength={2_000} placeholder="Descreva o comportamento que precisa ser corrigido, sem incluir credenciais." onChange={event => setDescription(event.target.value)} /></div>
          <div className="flex flex-wrap justify-end gap-2"><Button variant="ghost" className="cursor-pointer" disabled={disabled || (!conversationId && !description)} onClick={clearSelection}>Limpar seleção</Button><Button className="cursor-pointer" disabled={disabled || !selected} onClick={() => { void capture(); }}><Bug className="size-4" />{controller.pending ? "Preparando…" : "Preparar incidente"}</Button></div>
          <div className="space-y-3 border-t border-border pt-4"><div className="flex items-center justify-between gap-3"><h3 className="micro-label text-muted-foreground">Incidentes compartilhados</h3><Badge variant="secondary" className="font-mono text-[10px]">{controller.incidents.length}</Badge></div>
            {!controller.incidents.length && <p className="text-xs text-muted-foreground">Prepare um incidente e copie sua referência para iniciar a investigação no chat deste projeto Jarvis.</p>}
            {controller.incidents.map(incident => <Card key={incident.id} className="gap-3 p-4">
              <div className="flex min-w-0 items-start justify-between gap-3"><div className="min-w-0"><h4 className="truncate text-sm font-medium">{incident.conversationTitle}</h4><p className="mt-1 truncate text-xs text-muted-foreground">{incident.sourceProjectName} · {sourceStatus(incident.sourceStatus)}</p></div><Badge variant="outline" className="shrink-0 font-mono text-[10px]">{incident.eventCount} eventos{incident.truncated ? " · parcial" : ""}</Badge></div>
              <p className="font-mono text-[10px] text-muted-foreground">{new Date(incident.capturedAt).toLocaleString("pt-BR")}</p>
              <p className="break-words rounded-md border border-border bg-secondary/50 p-3 font-mono text-[11px] leading-relaxed">{incident.reference}</p>
              <div className="flex flex-wrap justify-end gap-2"><Button variant="ghost" size="sm" className="cursor-pointer text-destructive" disabled={disabled} onClick={() => setRevoking(incident)}><Trash2 className="size-3.5" />Revogar e remover</Button><Button variant="outline" size="sm" className="cursor-pointer" disabled={disabled || copying !== null} onClick={() => { void copy(incident); }}><Copy className="size-3.5" />{copying === incident.id ? "Copiando…" : "Copiar referência para o chat"}</Button></div>
            </Card>)}
          </div>
        </>}
      </CardContent>
    </Card>}

    <AlertDialog open={disabling} onOpenChange={setDisabling}><AlertDialogContent><AlertDialogHeader><AlertDialogTitle>Desativar o ambiente do Jarvis?</AlertDialogTitle><AlertDialogDescription>Todos os incidentes deste projeto serão removidos deste dispositivo. Os agentes perderão acesso aos diagnósticos compartilhados.</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel className="cursor-pointer" disabled={controller.pending}>Cancelar</AlertDialogCancel><AlertDialogAction className="cursor-pointer" variant="destructive" disabled={controller.pending} onClick={() => { void toggle(false); }}>Desativar e revogar</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>
    <AlertDialog open={enabled && revoking !== null} onOpenChange={open => { if (!open) setRevoking(null); }}><AlertDialogContent><AlertDialogHeader><AlertDialogTitle>Revogar este incidente?</AlertDialogTitle><AlertDialogDescription>O incidente será removido deste dispositivo e deixará de estar disponível aos agentes deste projeto.</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel className="cursor-pointer" disabled={controller.pending}>Cancelar</AlertDialogCancel><AlertDialogAction className="cursor-pointer" variant="destructive" disabled={controller.pending} onClick={() => { if (revoking) void controller.remove(revoking.id).then(removed => { if (removed) { setRevoking(null); toast.success("Incidente removido e acesso revogado"); } }); }}>Revogar incidente</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>
  </div>;
}
