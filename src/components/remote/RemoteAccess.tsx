import { useState } from "react";
import { Copy, RefreshCw, Smartphone, Unplug, Wifi } from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { toast } from "sonner";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Hint } from "@/components/ui/hint";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { writeClipboardText } from "@/core/clipboard";
import { pairingForAddress, remoteControlError } from "@/core/remote-control";
import { useRemoteControl } from "@/hooks/use-remote-control";
import { cn } from "@/lib/utils";

export function RemoteAccess() {
  const [open, setOpen] = useState(false);
  const [address, setAddress] = useState<string | null>(null);
  const control = useRemoteControl(open);
  const { status, busy, error } = control;
  const selectedAddress = status?.urls.includes(address ?? "") ? address! : status?.urls[0] ?? null;
  const pairing = status && selectedAddress ? pairingForAddress(status, selectedAddress) : null;
  const problem = error ?? status?.error;
  const copy = async () => {
    if (!pairing) return;
    try { await writeClipboardText(pairing); toast.success("Link de pareamento copiado"); }
    catch (cause) { toast.error(remoteControlError(cause)); }
  };
  const title = status?.running ? "Modo remoto ativo" : "Modo remoto";

  return <>
    <Hint content={title}><Button variant="ghost" size="icon-sm" className={cn("h-6 w-7 shrink-0 cursor-pointer rounded-sm", status?.running ? "text-onedark-cyan" : "text-muted-foreground")} aria-label="Modo remoto" onClick={() => setOpen(true)}><Smartphone className="size-3.5" /></Button></Hint>
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="max-h-[85dvh] overflow-y-auto overflow-x-hidden sm:max-w-lg">
        <DialogHeader className="pr-7"><DialogTitle>Modo remoto</DialogTitle><DialogDescription>Acompanhe as conversas e responda às pendências pelo celular, na mesma rede do computador.</DialogDescription></DialogHeader>
        {!status ? <div role="status" aria-label="Carregando acesso remoto" className="flex flex-col gap-3"><Skeleton className="h-16" /><Skeleton className="h-52" /></div> : <>
          <Field orientation="horizontal" className="rounded-lg border border-border p-3">
            <FieldContent><FieldLabel htmlFor="remote-enabled">Acesso pela rede local</FieldLabel><FieldDescription>Mantenha o Jarvis aberto e o computador ligado.</FieldDescription></FieldContent>
            <Switch id="remote-enabled" className="cursor-pointer" checked={status.enabled} disabled={busy} onCheckedChange={enabled => { void control.run("set_remote_enabled", { enabled }).then(saved => { if (saved) toast.success(enabled ? "Modo remoto ativado" : "Modo remoto desativado"); }); }} />
          </Field>
          <div className="flex items-center justify-between gap-3"><Badge variant="outline" className={status.running ? "text-onedark-green" : "text-muted-foreground"}><Wifi className="size-3" />{status.running ? "Serviço ativo" : status.enabled ? "Serviço indisponível" : "Desativado"}</Badge><Button variant="ghost" size="sm" disabled={busy} className="cursor-pointer" onClick={() => void control.refresh()}><RefreshCw data-icon="inline-start" />Verificar status</Button></div>
          {status.running && <Card size="sm">
            <CardHeader><CardTitle>Conectar celular</CardTitle><CardDescription>Leia o QRCode com a câmera do celular. O link autoriza um aparelho e expira em cinco minutos.</CardDescription></CardHeader>
            <CardContent className="flex flex-col gap-3">
              {status.urls.length > 1 && <Field><FieldLabel htmlFor="remote-address">Endereço da rede</FieldLabel><Select value={selectedAddress} onValueChange={setAddress}><SelectTrigger id="remote-address" className="w-full cursor-pointer"><SelectValue>{selectedAddress}</SelectValue></SelectTrigger><SelectContent><SelectGroup>{status.urls.map(url => <SelectItem className="cursor-pointer" key={url} value={url}>{url}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>}
              {pairing ? <>
                <figure className="flex flex-col items-center gap-3"><div className="rounded-lg bg-white p-3"><QRCodeSVG value={pairing} size={180} level="M" marginSize={2} title="QRCode para parear o celular ao Jarvis" /></div><figcaption className="break-all text-center font-mono text-xs text-muted-foreground">{selectedAddress}</figcaption></figure>
                {status.pairingExpiresAt && <p className="text-center text-xs text-muted-foreground">Válido até {new Date(status.pairingExpiresAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}</p>}
              </> : <p className="text-sm text-muted-foreground">Conecte o computador à rede local para gerar o QRCode.</p>}
              <div className="flex flex-wrap justify-center gap-2"><Button variant="outline" size="sm" className="cursor-pointer" disabled={busy || !pairing} onClick={() => void copy()}><Copy data-icon="inline-start" />Copiar link</Button><Button variant="ghost" size="sm" className="cursor-pointer" disabled={busy} onClick={() => void control.run("refresh_remote_pairing")}><RefreshCw data-icon="inline-start" />Novo QRCode</Button></div>
              <p className="text-xs leading-5 text-muted-foreground">Use uma rede confiável. Este acesso usa HTTP local; o tráfego não é criptografado.</p>
            </CardContent>
          </Card>}
          {status.running && <Card size="sm"><CardHeader><CardTitle>Aparelhos autorizados <span className="font-mono text-muted-foreground">{status.devices.length}</span></CardTitle><CardDescription>Revogue um aparelho para encerrar o acesso dele.</CardDescription></CardHeader><CardContent className="flex flex-col gap-2">{status.devices.length ? status.devices.map(device => <div key={device.id} className="flex min-w-0 items-center gap-3 rounded-md border border-border p-2"><Smartphone className="size-4 shrink-0 text-muted-foreground" /><div className="min-w-0 flex-1"><p className="truncate">{device.name}</p><p className="text-xs text-muted-foreground">Último acesso às {new Date(device.lastSeenAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}</p></div><Hint content={`Revogar ${device.name}`}><Button variant="ghost" size="icon-sm" className="cursor-pointer text-destructive" aria-label={`Revogar ${device.name}`} disabled={busy} onClick={() => void control.run("revoke_remote_device", { deviceId: device.id }).then(saved => { if (saved) toast.success("Acesso do aparelho revogado"); })}><Unplug /></Button></Hint></div>) : <p className="text-sm text-muted-foreground">Nenhum aparelho conectado.</p>}</CardContent></Card>}
        </>}
        {problem && <Alert variant="destructive"><AlertDescription>{problem}</AlertDescription></Alert>}
        {!status && error && <Button variant="outline" className="cursor-pointer" onClick={() => void control.refresh()}>Tentar novamente</Button>}
      </DialogContent>
    </Dialog>
  </>;
}
