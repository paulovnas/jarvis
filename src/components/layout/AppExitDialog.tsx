import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { ConfirmationDialogContent } from "@/components/ConfirmationDialogContent";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { cancelAppExit, confirmAppExit, getPendingAppExit } from "@/core/app-exit";
import { appShutdownStatusSchema, nativeUpdaterAvailable, type AppShutdownStatus } from "@/core/app-update";

const failureMessage = (cause: unknown) => typeof cause === "string" ? cause : "Não foi possível fechar o Jarvis. Tente novamente.";

export function AppExitDialog() {
  const [status, setStatus] = useState<AppShutdownStatus | null>(null);
  const [action, setAction] = useState<"confirm" | "cancel" | null>(null);
  const busy = action !== null;
  const flight = useRef(false);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    let active = true;
    let revision = 0;
    const disposers: (() => void)[] = [];
    const register = async (event: "app:exit-requested" | "app:exit-error") => {
      const unlisten = await listen<unknown>(event, message => {
        if (!active) return;
        if (event === "app:exit-error") { toast.error(failureMessage(message.payload)); return; }
        const next = appShutdownStatusSchema.safeParse(message.payload);
        if (!next.success) { toast.error("Não foi possível verificar os processos ativos."); return; }
        revision += 1;
        setStatus(next.data);
      });
      if (active) disposers.push(unlisten);
      else unlisten();
    };
    if (nativeUpdaterAvailable()) void Promise.allSettled([register("app:exit-requested"), register("app:exit-error")]).then(async results => {
      if (!active) return;
      if (results.some(result => result.status === "rejected")) { toast.error("Não foi possível acompanhar o fechamento do Jarvis."); return; }
      const request = revision;
      try {
        const pending = await getPendingAppExit();
        if (active && request === revision) setStatus(pending);
      } catch (cause) { if (active) toast.error(failureMessage(cause)); }
    });
    return () => { active = false; mounted.current = false; disposers.forEach(dispose => dispose()); };
  }, []);

  const resolveExit = async (confirm: boolean) => {
    if (flight.current || !status) return;
    flight.current = true; setAction(confirm ? "confirm" : "cancel");
    try {
      if (confirm) await confirmAppExit();
      else await cancelAppExit();
      if (mounted.current) setStatus(null);
    } catch (cause) { if (mounted.current) toast.error(failureMessage(cause)); }
    finally { flight.current = false; if (mounted.current) setAction(null); }
  };

  return <AlertDialog open={status !== null} onOpenChange={next => { if (!next) void resolveExit(false); }}>
    <ConfirmationDialogContent className="dark">
      <AlertDialogHeader>
        <AlertDialogTitle>Fechar o Jarvis?</AlertDialogTitle>
        <AlertDialogDescription>
          {!!status?.activeProcesses && "Os terminais e processos ativos serão encerrados. As abas dos terminais serão restauradas na próxima abertura, e os serviços de desenvolvimento elegíveis que ainda estiverem ativos serão reiniciados automaticamente. Comandos concluídos ou de execução única não serão repetidos."}
          {!!status?.activeChats && (status.activeProcesses ? " As execuções ativas dos chats também serão interrompidas." : "As execuções ativas dos chats serão interrompidas.")}
          {status?.activeChats === 0 && status.activeProcesses === 0 && "O Jarvis será fechado."}
        </AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <AlertDialogCancel className="cursor-pointer" disabled={busy}>{action === "cancel" ? "Cancelando…" : "Cancelar"}</AlertDialogCancel>
        <AlertDialogAction data-confirm-action className="cursor-pointer" disabled={busy} onClick={() => { void resolveExit(true); }}>{action === "confirm" ? "Fechando…" : "Encerrar e fechar"}</AlertDialogAction>
      </AlertDialogFooter>
    </ConfirmationDialogContent>
  </AlertDialog>;
}
