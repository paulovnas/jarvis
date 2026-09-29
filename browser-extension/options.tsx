import { useEffect, useState } from "react";
import { Globe, Link2, Unplug } from "lucide-react";
import { toast, Toaster } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardAction, CardFooter } from "@/components/ui/card";
import { FieldGroup, Field, FieldLabel } from "@/components/ui/field";
import { Textarea } from "@/components/ui/textarea";
import { Badge } from "@/components/ui/badge";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { object, pairingSchema } from "./protocol";

export function Options() {
  const [code, setCode] = useState("");
  const [status, setStatus] = useState({ state: "disconnected", message: "Cole o código de conexão fornecido pelo Jarvis." });
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    const update = (value: unknown) => {
      const data = object(value);
      if (typeof data.state === "string" && typeof data.message === "string") setStatus({ state: data.state, message: data.message });
    };
    void chrome.runtime.sendMessage({ type: "status" }).then(result => update(object(result).status)).catch(() => {});
    const listener = (changes: Record<string, chrome.storage.StorageChange>, area: string) => {
      if (area === "session" && changes.connectionStatus) update(changes.connectionStatus.newValue);
    };
    chrome.storage.onChanged.addListener(listener);
    return () => chrome.storage.onChanged.removeListener(listener);
  }, []);

  async function configure(type: "connect" | "disconnect") {
    setBusy(true);
    try {
      let pairing: unknown;
      if (type === "connect") {
        try { pairing = pairingSchema.parse(JSON.parse(code)); }
        catch { throw new Error("Código inválido. Copie o código completo nas configurações do Jarvis."); }
      }
      const result = object(await chrome.runtime.sendMessage({ type, ...(pairing ? { pairing } : {}) }));
      if (!result.ok) throw new Error(String(result.error || "Não foi possível atualizar a conexão."));
      setCode("");
      toast.success(type === "connect" ? "Conexão configurada" : "Navegador desconectado");
    } catch (error) { toast.error(error instanceof Error ? error.message : "Falha ao configurar a conexão."); }
    finally { setBusy(false); }
  }

  return <main className="mx-auto flex w-full max-w-lg flex-col gap-6 px-5 py-10 sm:py-16">
    <header className="flex items-center gap-3">
      <img src="./icons/64.png" alt="Jarvis" width={44} height={44} className="size-11 shrink-0" />
      <div className="flex min-w-0 flex-col gap-1"><h1 className="text-xl font-semibold">Jarvis no navegador</h1><p className="text-sm text-muted-foreground">Suas páginas, conectadas ao trabalho dos agentes.</p></div>
    </header>
    <Card>
      <CardHeader>
        <CardTitle>Conexão local</CardTitle>
        <CardDescription>No aplicativo, abra Configurações → Navegador e copie o código de conexão.</CardDescription>
        <CardAction><Badge variant={status.state === "connected" ? "default" : "secondary"}>{status.state === "connected" ? "Conectado" : status.state === "connecting" ? "Conectando" : status.state === "error" ? "Aguardando Jarvis" : "Desconectado"}</Badge></CardAction>
      </CardHeader>
      <CardContent>
        <form onSubmit={event => { event.preventDefault(); void configure("connect"); }}>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="connection-code">Código de conexão</FieldLabel>
              <Textarea id="connection-code" value={code} disabled={busy} onChange={event => setCode(event.target.value)} className="min-h-24 resize-y font-mono text-xs" spellCheck={false} autoComplete="off" placeholder="Cole o código aqui" />
            </Field>
            <div className="flex flex-wrap items-center gap-2">
              <Button type="submit" className="cursor-pointer" disabled={busy || !code.trim()}><Link2 data-icon="inline-start" />Conectar</Button>
              <Button type="button" variant="outline" className="cursor-pointer" disabled={busy || status.state === "disconnected"} onClick={() => void configure("disconnect")}><Unplug data-icon="inline-start" />Desconectar</Button>
            </div>
          </FieldGroup>
        </form>
      </CardContent>
      <CardFooter><Alert role="status" aria-live="polite"><Globe /><AlertDescription>{status.message}</AlertDescription></Alert></CardFooter>
    </Card>
    <p className="text-xs leading-relaxed text-muted-foreground">O Jarvis controla as abas que você conectar e pode abrir novas páginas. Durante o uso, o navegador exibe seu aviso de depuração. Desconectar preserva suas abas.</p>
    <Toaster theme="dark" richColors />
  </main>;
}
