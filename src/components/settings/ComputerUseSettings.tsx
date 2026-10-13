import { Accessibility, Camera, Keyboard, Monitor, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { RowsSkeleton } from "@/components/layout/LoadingSkeletons";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useComputerUse } from "@/hooks/use-computer-use";
import { computerModeDescriptions, type ComputerPermission } from "@/core/computer-use";

const permissions = [
  { key: "accessibility", name: "Acessibilidade", description: "Ler a estrutura e interagir com os controles dos aplicativos autorizados.", action: "Permitir acessibilidade", Icon: Accessibility },
  { key: "screenCapture", name: "Captura de tela", description: "Capturar somente a janela selecionada do aplicativo autorizado.", action: "Permitir captura de tela", Icon: Camera },
  { key: "inputPosting", name: "Envio de entradas", description: "Enviar cliques, teclas, rolagem e arrasto aos aplicativos autorizados.", action: "Permitir envio de entradas", Icon: Keyboard },
] satisfies { key: ComputerPermission; name: string; description: string; action: string; Icon: typeof Monitor }[];

export function ComputerUseSettings() {
  const computer = useComputerUse();
  const capabilities = computer.snapshot?.capabilities;
  const request = async (permission: ComputerPermission) => {
    if (await computer.requestPermission(permission)) toast.info("Solicitação enviada ao macOS. Atualize o estado após conceder a permissão.");
  };
  return <section id="computer-use-settings" tabIndex={-1} aria-label="Uso do computador" className="space-y-3 scroll-mt-6 outline-none">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <h2 className="flex items-center gap-2 text-sm font-medium"><Monitor aria-hidden="true" className="size-4 text-muted-foreground" />Uso do computador</h2>
      <Button type="button" variant="ghost" size="sm" className="cursor-pointer" disabled={!!computer.busy} onClick={() => void computer.refresh()} aria-label="Atualizar permissões"><RefreshCw className="size-3.5" />Atualizar estado</Button>
    </div>
    <p className="text-xs leading-5 text-muted-foreground">Escolha como o agente usa o computador e conceda as permissões do macOS uma vez para o Jarvis.</p>
    {computer.error && <p role="alert" className="text-xs text-destructive">{computer.error}</p>}
    {!capabilities && !computer.error && <Card className="settings-group"><CardContent className="p-0"><RowsSkeleton label="Carregando permissões do computador" count={3} /></CardContent></Card>}
    {capabilities && !capabilities.supported && <Card className="settings-group"><CardContent className="p-4"><p role="status" className="text-sm">{capabilities.reason ?? "O uso nativo do computador está disponível apenas no macOS."}</p><p className="mt-1 font-mono text-[10px] text-muted-foreground">{capabilities.platform}</p></CardContent></Card>}
    {capabilities?.supported && <Card className="settings-group"><CardContent className="divide-y divide-border p-0">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3.5">
        <Monitor aria-hidden="true" className="size-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1 basis-56"><Label htmlFor="computer-use-mode" className="text-sm">Modo de uso do computador</Label><p className="mt-1 text-xs leading-5 text-muted-foreground">{computerModeDescriptions[computer.snapshot?.mode ?? "manual"]}</p></div>
        <Select value={computer.snapshot?.mode ?? "manual"} items={[{ value: "manual", label: "Manual" }, { value: "yolo", label: "YOLO" }]} disabled={!!computer.busy} onValueChange={mode => { if (mode) void computer.setMode(mode).then(saved => { if (saved) toast.success("Modo de uso do computador salvo"); }); }}>
          <SelectTrigger id="computer-use-mode" aria-label="Modo de uso do computador" className="w-40 cursor-pointer"><SelectValue /></SelectTrigger>
          <SelectContent><SelectItem value="manual" className="cursor-pointer">Manual</SelectItem><SelectItem value="yolo" className="cursor-pointer">YOLO</SelectItem></SelectContent>
        </Select>
      </div>
      {permissions.map(({ key, name, description, action, Icon }) => <div key={key} className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3.5">
        <Icon aria-hidden="true" className="size-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1 basis-56"><h3 className="text-sm font-medium">{name}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{description}</p>{key === "screenCapture" && !capabilities.captureSupported && <p className="mt-1 text-xs text-onedark-yellow">A captura de janelas está indisponível nesta sessão.</p>}</div>
        <Badge variant="outline" className="text-xs font-normal">{capabilities[key] ? "Concedida" : "Não concedida"}</Badge>
        {!capabilities[key] && <Button type="button" variant="outline" size="sm" className="cursor-pointer text-xs" disabled={!!computer.busy || key === "screenCapture" && !capabilities.captureSupported} onClick={() => void request(key)}>{action}</Button>}
      </div>)}
      {capabilities.reason && <p role="status" className="px-4 py-3 text-xs text-onedark-yellow">{capabilities.reason}</p>}
    </CardContent></Card>}
  </section>;
}
