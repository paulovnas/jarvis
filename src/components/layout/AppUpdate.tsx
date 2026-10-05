import { useEffect, useState, type ReactNode } from "react";
import { ArrowUpToLine, Check, CircleAlert, CircleCheck, Copy, ExternalLink, RefreshCw } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { JarvisLogo } from "@/components/JarvisLogo";
import { LazyChatMarkdown } from "@/components/chat/LazyChatMarkdown";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { ConfirmationDialogContent } from "@/components/ConfirmationDialogContent";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import { displayVersion, nativeUpdaterAvailable, PROJECT_URL } from "@/core/app-update";
import { writeClipboardText } from "@/core/clipboard";
import { useAppUpdate } from "@/hooks/use-app-update";
import { cn } from "@/lib/utils";
import { openAuxiliaryWindow } from "@/core/auxiliary-windows";

function megabytes(bytes: number): string { return `${(bytes / 1024 / 1024).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} MB`; }

const PIX_COPY_AND_PASTE = "00020101021126540014br.gov.bcb.pix0132nascimento.paulo.vitor@gmail.com5204000053039865802BR5923PAULO V A DE O NASCIMEN6006AMPARO62070503***6304B333";

function AboutSurface({ standalone, open, busy, onOpenChange, trigger, children }: { standalone: boolean; open: boolean; busy: boolean; onOpenChange: (open: boolean) => void; trigger: ReactNode; children: ReactNode }) {
  if (standalone) return <Card role="region" aria-label="Sobre o Jarvis" className="flex h-full min-h-0 flex-1 flex-col gap-0 overflow-hidden rounded-none border-0 py-0 shadow-none">{children}</Card>;
  return <Dialog open={open} onOpenChange={onOpenChange}>{trigger}<DialogContent className="dark flex max-h-[calc(100dvh-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-xl" showCloseButton={!busy}>{children}</DialogContent></Dialog>;
}

export function AppUpdate({ standalone = false, onBusyChange }: { standalone?: boolean; onBusyChange?: (busy: boolean) => void }) {
  const [open, setOpen] = useState(false);
  const [pixCopied, setPixCopied] = useState(false);
  const { info, checking, busy, progress, error, upToDate, pendingShutdown, check, install, confirmInstall, cancelInstall } = useAppUpdate();
  const Description = standalone ? CardDescription : DialogDescription;
  useEffect(() => { onBusyChange?.(busy || pendingShutdown !== null); }, [busy, pendingShutdown, onBusyChange]);
  useEffect(() => {
    if (standalone || !nativeUpdaterAvailable()) return;
    let active = true;
    let dispose: (() => void) | undefined;
    void listen("app:about", () => { if (active) void openAuxiliaryWindow("about").then(native => { if (!native && active) setOpen(true); }).catch(() => toast.error("Não foi possível abrir Sobre o Jarvis.")); }).then(unlisten => {
      if (active) dispose = unlisten;
      else unlisten();
    }).catch(() => {});
    return () => { active = false; dispose?.(); };
  }, [standalone]);
  const release = info.available;
  const downloaded = progress?.stage === "downloading" ? progress.downloaded : 0;
  const total = progress?.stage === "downloading" ? progress.total : null;
  const percent = total && total > 0 ? Math.min(100, Math.round(downloaded / total * 100)) : null;
  const installed = progress?.stage === "restarting";
  const stage = !progress ? "Preparando atualização" : progress.stage === "downloading" ? "Baixando atualização" : progress.stage === "verifying" ? "Verificando assinatura" : progress.stage === "installing" ? "Instalando atualização" : "Reabrindo o Jarvis";
  const copyPix = async () => {
    try {
      await writeClipboardText(PIX_COPY_AND_PASTE);
      setPixCopied(true);
      toast.success("PIX copia e cola copiado");
    } catch {
      toast.error("Não foi possível copiar o PIX.");
    }
  };
  const aboutIdentity = <div className="flex items-center gap-5">
    <JarvisLogo variant="vertical" className="size-24 shrink-0" />
    <div className="flex min-w-0 flex-col gap-2">
      {!standalone && <DialogTitle>{release ? "Atualizar Jarvis" : "Sobre o Jarvis"}</DialogTitle>}
      <Description>{release ? "Uma nova versão está disponível para você." : "Ambiente de desenvolvimento com agentes de IA."}</Description>
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant="outline" className="font-mono text-[10px]">{displayVersion(info.currentVersion)}</Badge>
        {release && <><span aria-hidden="true" className="text-muted-foreground">→</span><Badge variant="secondary" className="font-mono text-[10px]">{displayVersion(release.version)}</Badge></>}
      </div>
    </div>
  </div>;
  const versionButton = <Button variant="ghost" size="sm" className={cn("h-6 shrink-0 cursor-pointer rounded-sm px-1.5 font-mono text-[10px]", release ? "text-onedark-green" : "text-muted-foreground")} aria-label={release ? "Atualização Disponível" : `Sobre o Jarvis ${displayVersion(info.currentVersion)}`} onClick={() => { void openAuxiliaryWindow("about").then(native => { if (!native) setOpen(true); }).catch(() => toast.error("Não foi possível abrir Sobre o Jarvis.")); }}>{release ? "Atualização Disponível" : displayVersion(info.currentVersion)}</Button>;
  if (!standalone && "__TAURI_INTERNALS__" in window) return versionButton;
  return <><AboutSurface standalone={standalone} open={open} busy={busy} onOpenChange={next => { if (!busy) setOpen(next); }} trigger={<DialogTrigger render={versionButton} />}>
      {!standalone && <><DialogHeader className="shrink-0 p-5 pr-10 sm:p-6 sm:pr-10">{aboutIdentity}</DialogHeader><Separator /></>}
      <section aria-label={release ? "Notas da versão" : "Sobre o projeto"} tabIndex={0} className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto overscroll-contain p-5 outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring sm:p-6">
        {standalone && aboutIdentity}
        {release ? <>
          {release.notes ? <LazyChatMarkdown content={release.notes} /> : <p className="text-sm text-muted-foreground">Nova versão disponível.</p>}
          {!info.installable && <p className="text-xs text-muted-foreground">A atualização automática não está disponível nesta instalação. Baixe a nova versão; no Linux, instale o novo DEB ou use um AppImage em uma pasta com permissão de escrita.</p>}
        </> : <>
          <div className="flex flex-col gap-3">
            <p className="text-sm leading-relaxed text-muted-foreground">Um projeto sem fins lucrativos para tornar o desenvolvimento com IA mais acessível, com autonomia e controle.</p>
            <p className="text-xs text-muted-foreground">Criado por <span className="font-medium text-foreground">Paulo Vitor Nascimento</span></p>
          </div>
          <Card size="sm" className="shrink-0 gap-4 sm:grid sm:grid-cols-[minmax(0,1fr)_160px] sm:items-center">
            <CardHeader className="gap-2 sm:pr-0">
              <CardTitle><h3>Compre-me um açaí 🫐</h3></CardTitle>
              <CardDescription>Se o Jarvis ajuda no seu dia a dia, uma contribuição é sempre bem-vinda para manter o desenvolvimento ativo.</CardDescription>
              <div className="pt-2">
                <Button type="button" variant="outline" size="sm" className="cursor-pointer" onClick={() => void copyPix()} onBlur={() => setPixCopied(false)}>
                  {pixCopied ? <Check aria-hidden="true" data-icon="inline-start" /> : <Copy aria-hidden="true" data-icon="inline-start" />}
                  {pixCopied ? "PIX copiado" : "Copiar PIX copia e cola"}
                </Button>
              </div>
            </CardHeader>
            <CardContent className="flex flex-col items-center gap-2 sm:pl-0">
              <div className="shrink-0 rounded-lg bg-white p-2"><img src="/acai.png" alt="QR Code para apoiar o Jarvis via PIX" className="size-32" /></div>
              <p className="max-w-40 text-center text-[11px] leading-4 text-muted-foreground">Leia no app do banco e escolha o valor.</p>
            </CardContent>
          </Card>
        </>}
      </section>
      <DialogFooter className="mx-0 mb-0 shrink-0 flex-col gap-3 p-5 sm:flex-col sm:p-6">
        {(busy || checking) && <div role="status" aria-label={checking ? "Verificando atualizações" : stage} aria-live="polite" className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-3 text-xs"><span>{checking ? "Verificando atualizações" : stage}</span>{progress?.stage === "downloading" && <span className="font-mono tabular-nums">{percent === null ? megabytes(downloaded) : `${percent}%`}</span>}</div>
          <Progress aria-label={checking ? "Verificando atualizações" : stage} value={progress?.stage === "downloading" ? percent : null} />
          {progress?.stage === "downloading" && total && <p className="font-mono text-[10px] text-muted-foreground">{megabytes(downloaded)} / {megabytes(total)}</p>}
        </div>}
        {error && <Alert variant="destructive"><CircleAlert /><AlertDescription className="max-h-24 overflow-y-auto">{error}</AlertDescription></Alert>}
        {upToDate && <Alert role="status" className="border-onedark-green/25 bg-onedark-green/5 text-onedark-green"><CircleCheck /><AlertDescription className="text-onedark-green">A versão mais recente já está instalada.</AlertDescription></Alert>}
        {release && info.installable && !busy && !installed && <p className="text-xs text-muted-foreground">O Jarvis será reaberto automaticamente após a instalação.</p>}
        <div className="flex flex-wrap items-center justify-between gap-2">
          <Button variant="ghost" size="sm" className="cursor-pointer" onClick={() => { void openUrl(PROJECT_URL).catch(() => toast.error("Não foi possível abrir o projeto.")); }}><ExternalLink data-icon="inline-start" />GitHub</Button>
          {release ? info.installable ? <Button className="cursor-pointer" disabled={busy || checking} onClick={() => void install()}><ArrowUpToLine data-icon="inline-start" />{busy ? stage : installed ? "Reabrir Jarvis" : "Atualizar e reiniciar"}</Button> : <Button className="cursor-pointer" onClick={() => { void openUrl(`${PROJECT_URL}/releases`).catch(() => toast.error("Não foi possível abrir os downloads.")); }}><ExternalLink data-icon="inline-start" />Baixar nova versão</Button> : <Button variant="outline" className="cursor-pointer" disabled={checking} onClick={() => void check(true)}><RefreshCw data-icon="inline-start" />Verificar atualizações</Button>}
        </div>
      </DialogFooter>
  </AboutSurface>
    <AlertDialog open={pendingShutdown !== null} onOpenChange={next => { if (!next) cancelInstall(); }}>
      <ConfirmationDialogContent className="dark">
        <AlertDialogHeader>
          <AlertDialogTitle>Atualizar e encerrar processos?</AlertDialogTitle>
          <AlertDialogDescription>
            {pendingShutdown?.activeProcesses === 1 ? "Um processo ativo será encerrado" : `${pendingShutdown?.activeProcesses ?? 0} processos ativos serão encerrados`} para atualizar o Jarvis. Os terminais serão restaurados na próxima abertura.
            {pendingShutdown?.restartableProcesses ? ` ${pendingShutdown.restartableProcesses === 1 ? "Um serviço de desenvolvimento será reiniciado" : `${pendingShutdown.restartableProcesses} serviços de desenvolvimento serão reiniciados`} automaticamente.` : " Serviços de desenvolvimento elegíveis que ainda estiverem ativos serão reiniciados automaticamente."}
            {" Comandos concluídos ou de execução única não serão repetidos."}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel className="cursor-pointer">Cancelar</AlertDialogCancel>
          <AlertDialogAction data-confirm-action className="cursor-pointer" onClick={() => { void confirmInstall(); }}>Encerrar e atualizar</AlertDialogAction>
        </AlertDialogFooter>
      </ConfirmationDialogContent>
    </AlertDialog>
  </>;
}
