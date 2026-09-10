import { useEffect, useState } from "react";
import { ArrowUpToLine, Check, CircleAlert, CircleCheck, Copy, ExternalLink, RefreshCw } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { JarvisLogo } from "@/components/JarvisLogo";
import { LazyChatMarkdown } from "@/components/chat/LazyChatMarkdown";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { Skeleton } from "@/components/ui/skeleton";
import { displayVersion, nativeUpdaterAvailable, PROJECT_URL } from "@/core/app-update";
import { useAppUpdate } from "@/hooks/use-app-update";
import { cn } from "@/lib/utils";

function megabytes(bytes: number): string { return `${(bytes / 1024 / 1024).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} MB`; }

const PIX_COPY_AND_PASTE = "00020101021126540014br.gov.bcb.pix0132nascimento.paulo.vitor@gmail.com5204000053039865802BR5923PAULO V A DE O NASCIMEN6006AMPARO62070503***6304B333";

export function AppUpdate() {
  const [open, setOpen] = useState(false);
  const [pixCopied, setPixCopied] = useState(false);
  const { info, checking, busy, progress, error, upToDate, check, install } = useAppUpdate();
  useEffect(() => {
    if (!nativeUpdaterAvailable()) return;
    let active = true;
    let dispose: (() => void) | undefined;
    void listen("app:about", () => { if (active) setOpen(true); }).then(unlisten => {
      if (active) dispose = unlisten;
      else unlisten();
    }).catch(() => {});
    return () => { active = false; dispose?.(); };
  }, []);
  const release = info.available;
  const downloaded = progress?.stage === "downloading" ? progress.downloaded : 0;
  const total = progress?.stage === "downloading" ? progress.total : null;
  const percent = total && total > 0 ? Math.min(100, Math.round(downloaded / total * 100)) : null;
  const installed = progress?.stage === "restarting";
  const stage = !progress ? "Preparando atualização" : progress.stage === "downloading" ? "Baixando atualização" : progress.stage === "verifying" ? "Verificando assinatura" : progress.stage === "installing" ? "Instalando atualização" : "Reabrindo o Jarvis";
  const copyPix = async () => {
    try {
      await navigator.clipboard.writeText(PIX_COPY_AND_PASTE);
      setPixCopied(true);
      toast.success("PIX copia e cola copiado");
    } catch {
      toast.error("Não foi possível copiar o PIX.");
    }
  };
  return <Dialog open={open} onOpenChange={next => { if (!busy) setOpen(next); }}>
    <DialogTrigger render={<Button variant="ghost" size="sm" />} className={cn("h-6 shrink-0 cursor-pointer rounded-sm px-1.5 font-mono text-[10px]", release ? "text-onedark-green" : "text-muted-foreground")} aria-label={release ? "Atualização Disponível" : `Sobre o Jarvis ${displayVersion(info.currentVersion)}`}>
      {release ? "Atualização Disponível" : displayVersion(info.currentVersion)}
    </DialogTrigger>
    <DialogContent className="dark flex max-h-[80vh] flex-col overflow-hidden sm:max-w-lg" showCloseButton={!busy} aria-describedby={undefined}>
      <DialogHeader className="shrink-0 items-center gap-1 text-center">
        <JarvisLogo variant="vertical" className="size-40 shrink-0" />
        <DialogTitle className={release ? "text-base" : "sr-only"}>{release ? "Atualizar Jarvis" : "Sobre o Jarvis"}</DialogTitle>
        <Badge variant="outline" className="font-mono text-[10px]">{displayVersion(release?.version ?? info.currentVersion)}</Badge>
      </DialogHeader>
      <div className="flex min-h-0 flex-col gap-4 overflow-y-auto">
        {release ? <>
          <div className="flex flex-wrap items-center gap-2 font-mono text-xs text-muted-foreground"><span>{info.currentVersion}</span><span aria-hidden="true">→</span><span>{release.version}</span></div>
          {release.notes ? <LazyChatMarkdown content={release.notes} /> : <p className="text-sm text-muted-foreground">Nova versão disponível.</p>}
          {!info.installable && <p className="text-xs text-muted-foreground">Abra o Jarvis instalado no computador para atualizar.</p>}
          {!busy && !installed && <p className="text-xs text-muted-foreground">O Jarvis será reaberto automaticamente após a instalação.</p>}
        </> : <>
          <p className="text-sm text-muted-foreground">Ambiente de desenvolvimento com agentes de IA.</p>
          <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-sm"><dt className="text-muted-foreground">Criado por</dt><dd>Paulo Vitor Nascimento</dd><dt className="text-muted-foreground">Versão</dt><dd className="font-mono text-xs">{info.currentVersion}</dd></dl>
          <section aria-labelledby="jarvis-support-title" className="rounded-lg border border-onedark-purple/25 bg-onedark-purple/5 p-4 shadow-[inset_0_1px_0_#ffffff0a]">
            <h3 id="jarvis-support-title" className="text-sm font-semibold text-foreground">Compre-me um açaí 🫐</h3>
            <p className="mt-2 text-xs leading-5 text-muted-foreground">O Jarvis é um projeto sem fins lucrativos, criado para ajudar quem quer iniciar nessa aventura de desenvolver com assistência de IA da melhor forma possível. Se ele tem ajudado você, uma contribuição é sempre bem-vinda e ajuda a manter o desenvolvimento ativo.</p>
            <div className="mt-4 flex flex-col items-center gap-4 sm:flex-row sm:items-start">
              <div className="shrink-0 rounded-lg bg-white p-2 shadow-sm"><img src="/acai.png" alt="QR Code para apoiar o Jarvis via PIX" className="size-36 rounded-md" /></div>
              <div className="flex min-w-0 flex-1 flex-col items-center gap-3 sm:items-start">
                <p className="text-center text-xs leading-5 text-muted-foreground sm:text-left">Leia o QR Code no aplicativo do seu banco e escolha o valor da contribuição.</p>
                <Button type="button" variant="outline" className="w-full cursor-pointer gap-2 sm:w-auto" onClick={() => void copyPix()} onBlur={() => setPixCopied(false)}>
                  {pixCopied ? <Check aria-hidden="true" className="text-onedark-green" /> : <Copy aria-hidden="true" />}
                  {pixCopied ? "PIX copiado" : "Copiar PIX copia e cola"}
                </Button>
              </div>
            </div>
          </section>
          {checking && <div role="status" aria-label="Verificando atualizações" className="flex flex-col gap-2"><Skeleton className="h-3 w-40" /><Skeleton className="h-2 w-full" /></div>}
        </>}
        {busy && progress && <div role="status" aria-live="polite" className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-3 text-xs"><span>{stage}</span>{progress.stage === "downloading" && <span className="font-mono tabular-nums">{percent === null ? megabytes(downloaded) : `${percent}%`}</span>}</div>
          <Progress aria-label={stage} value={progress.stage === "downloading" ? percent : null} />
          {progress.stage === "downloading" && total && <p className="font-mono text-[10px] text-muted-foreground">{megabytes(downloaded)} / {megabytes(total)}</p>}
        </div>}
        {error && <Alert variant="destructive"><CircleAlert /><AlertDescription>{error}</AlertDescription></Alert>}
        {upToDate && <Alert role="status" className="border-onedark-green/25 bg-onedark-green/5 text-onedark-green"><CircleCheck /><AlertDescription className="text-onedark-green">A versão mais recente já está instalada.</AlertDescription></Alert>}
      </div>
      <DialogFooter className="shrink-0">
        {release ? <Button disabled={busy || checking || !info.installable} onClick={() => void install()}><ArrowUpToLine data-icon="inline-start" />{busy ? stage : installed ? "Reabrir Jarvis" : "Atualizar e reiniciar"}</Button> : <>
          <Button variant="ghost" onClick={() => { void openUrl(PROJECT_URL).catch(() => toast.error("Não foi possível abrir o projeto.")); }}><ExternalLink data-icon="inline-start" />GitHub</Button>
          <Button variant="outline" disabled={checking} onClick={() => void check(true)}><RefreshCw data-icon="inline-start" />Verificar atualizações</Button>
        </>}
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}
