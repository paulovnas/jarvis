import { useState } from "react";
import { Play, Save, Square, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/TextInput";
import { Field, FieldLabel } from "@/components/ui/field";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { ConfirmationDialogContent } from "@/components/ConfirmationDialogContent";
import { Hint } from "@/components/ui/hint";
import { Separator } from "@/components/ui/separator";
import { Badge } from "@/components/ui/badge";
import type { HttpRun } from "@/core/http-client";
import type { HttpClientController, HttpTab } from "@/hooks/use-http-client";
import { HttpRequestEditor } from "./HttpRequestEditor";
import { HttpResponse } from "./HttpResponse";
import { httpRunLabels } from "@/core/http-presentation";

export function HttpPanel({ http, tab, onAnalyze }: { http: HttpClientController; tab: HttpTab; onAnalyze: (run: HttpRun) => void }) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  const request = tab.request;
  const editingLocked = !!http.busy && http.busy !== "autosave";
  const running = http.snapshot?.runs.find(run => run.draftId === tab.draft.id && run.status === "running");
  const runs = http.snapshot?.runs ?? [];
  const selected = runs.find(run => run.id === tab.selectedRunId) ?? (tab.selectedRunId ? null : runs.find(run => run.draftId === tab.draft.id)) ?? null;
  const patch = (next: Partial<typeof request>) => http.edit(tab.draft.id, { ...request, ...next });
  const environments = http.snapshot?.settings.environments ?? [];
  const environment = environments.find(environment => environment.id === request.environmentId);
  const saved = http.snapshot?.savedRequests ?? [];
  const linkedSaved = saved.find(item => item.id === tab.draft.savedRequestId);
  return <div className="flex h-full min-w-0 flex-col overflow-y-auto p-4 sm:p-5">
    <div className="flex min-w-0 flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2"><Input aria-label="Nome da requisição" value={request.name} disabled={editingLocked} onChange={event => patch({ name: event.target.value })} className="min-w-32 flex-1" />{tab.dirty && <Badge variant="outline">Não salvo</Badge>}<Select value="" disabled={!!http.busy || !saved.length} onValueChange={id => { const item = saved.find(item => item.id === id); if (item) void http.openSaved(item); }}><SelectTrigger aria-label="Abrir requisição salva" className="w-48 cursor-pointer"><SelectValue placeholder="Requisições salvas" /></SelectTrigger><SelectContent><SelectGroup>{saved.map(item => <SelectItem key={item.id} value={item.id} className="cursor-pointer">{item.request.name}</SelectItem>)}</SelectGroup></SelectContent></Select><Button size="sm" variant="outline" disabled={!!http.busy || !!tab.conflict} className="cursor-pointer" onClick={() => void http.saveRequest(tab.draft.id)}><Save />Salvar no projeto</Button>{linkedSaved && <Hint content="Excluir requisição salva"><Button size="icon-sm" variant="ghost" disabled={!!http.busy} aria-label="Excluir requisição salva" className="cursor-pointer" onClick={() => setConfirmDelete(true)}><Trash2 /></Button></Hint>}</div>
      {tab.conflict && <Alert><AlertTitle>Esta requisição mudou em outra origem</AlertTitle><AlertDescription><p>Sua edição foi preservada. Escolha a versão atual ou guarde sua edição em outra aba antes de enviar.</p><div className="mt-2 flex flex-wrap gap-2"><Button size="sm" variant="outline" className="cursor-pointer" onClick={() => http.useRemote(tab.draft.id)}>Usar versão atual</Button><Button size="sm" variant="secondary" disabled={!!http.busy} className="cursor-pointer" onClick={() => void http.open({ ...request, name: `${request.name} — cópia` })}>Salvar cópia</Button></div></AlertDescription></Alert>}
      <Field><FieldLabel>Ambiente</FieldLabel><Select value={request.environmentId ?? "__shared__"} disabled={editingLocked} onValueChange={id => patch({ environmentId: id === "__shared__" ? null : id })}><SelectTrigger aria-label="Ambiente da requisição" className="w-full cursor-pointer sm:w-64"><SelectValue>{request.environmentId ? environment?.name ?? "Ambiente indisponível" : "Somente variáveis compartilhadas"}</SelectValue></SelectTrigger><SelectContent><SelectGroup><SelectItem value="__shared__" className="cursor-pointer">Somente variáveis compartilhadas</SelectItem>{environments.map(item => <SelectItem key={item.id} value={item.id} className="cursor-pointer">{item.name}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
      <form className="flex flex-wrap items-center gap-2" onSubmit={event => { event.preventDefault(); void http.send(tab.draft.id); }}><Select value={request.method} disabled={editingLocked} onValueChange={method => { if (method) patch({ method }); }}><SelectTrigger aria-label="Método HTTP" className="w-28 cursor-pointer font-mono"><SelectValue>{request.method}</SelectValue></SelectTrigger><SelectContent><SelectGroup>{["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].map(method => <SelectItem key={method} value={method} className="cursor-pointer font-mono">{method}</SelectItem>)}</SelectGroup></SelectContent></Select><Input aria-label="URL da requisição" placeholder="{{base_url}}/recurso" value={request.url} disabled={editingLocked} onChange={event => patch({ url: event.target.value })} className="min-w-40 flex-1 font-mono text-xs" />{running ? <Button type="button" variant="outline" disabled={!!http.busy} className="cursor-pointer" onClick={() => void http.cancel(running.id)}><Square />Cancelar</Button> : <Button type="submit" disabled={!!http.busy || !!tab.conflict || !request.url.trim()} className="cursor-pointer"><Play />Enviar</Button>}</form>
      {http.error && <Alert variant="destructive"><AlertDescription>{http.error}</AlertDescription></Alert>}
      <HttpRequestEditor projectId={http.projectId} request={request} disabled={editingLocked} onChange={next => http.edit(tab.draft.id, next)} />
      <div className="flex justify-end"><Button size="sm" variant="ghost" disabled={!!http.busy || !tab.dirty || !!tab.conflict} className="cursor-pointer" onClick={() => void http.saveDraft(tab.draft.id)}>Salvar rascunho</Button></div>
      <Separator />
      <Field><FieldLabel>Histórico de execuções</FieldLabel><Select value={selected?.id ?? ""} onValueChange={id => { if (id) http.selectRun(tab.draft.id, id); }}><SelectTrigger aria-label="Execução HTTP selecionada" className="w-full min-w-0 cursor-pointer"><SelectValue placeholder="Nenhuma execução">{selected ? `${selected.httpStatus ?? httpRunLabels[selected.status]} · ${selected.request.name} · ${new Date(selected.startedAt).toLocaleString("pt-BR")}` : undefined}</SelectValue></SelectTrigger><SelectContent><SelectGroup>{runs.map(run => <SelectItem key={run.id} value={run.id} className="cursor-pointer">{run.httpStatus ?? httpRunLabels[run.status]} · {run.request.name} · {new Date(run.startedAt).toLocaleString("pt-BR")}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
      <HttpResponse key={selected?.id ?? "empty"} conversationId={http.conversationId} run={selected} onAnalyze={onAnalyze} />
    </div>
    <AlertDialog open={confirmDelete} onOpenChange={setConfirmDelete}><ConfirmationDialogContent><AlertDialogHeader><AlertDialogTitle>Excluir requisição salva?</AlertDialogTitle><AlertDialogDescription>Remove “{linkedSaved?.request.name}” das requisições do projeto. O rascunho desta aba e o histórico de execuções serão preservados.</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel className="cursor-pointer">Cancelar</AlertDialogCancel><AlertDialogAction className="cursor-pointer" onClick={() => { setConfirmDelete(false); if (linkedSaved) void http.deleteRequest(linkedSaved); }}>Excluir</AlertDialogAction></AlertDialogFooter></ConfirmationDialogContent></AlertDialog>
  </div>;
}
