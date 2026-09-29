import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { Globe, Link2, Unplug } from "lucide-react";
import { toast, Toaster } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { Badge } from "@/components/ui/badge";
import { object, pairingSchema } from "./protocol";
import "@/index.css";

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

  return <main className="mx-auto w-full max-w-xl space-y-6 px-5 py-12">
    <div className="flex items-center gap-3"><Globe className="size-8 text-primary" /><div><h1 className="text-xl font-semibold">Jarvis no seu navegador</h1><p className="text-sm text-muted-foreground">Suas páginas, conectadas ao trabalho dos agentes.</p></div></div>
    <Card>
      <CardHeader><CardTitle>Conectar ao Jarvis</CardTitle><CardDescription>No Jarvis, abra Configurações → Navegador e copie o código de conexão.</CardDescription></CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2"><Label htmlFor="connection-code">Código de conexão</Label><Textarea id="connection-code" value={code} onChange={event => setCode(event.target.value)} className="min-h-28 resize-y font-mono text-xs" spellCheck={false} placeholder="Cole o código aqui" /></div>
        <div className="flex flex-wrap gap-2"><Button className="cursor-pointer" disabled={busy || !code.trim()} onClick={() => void configure("connect")}><Link2 />Conectar</Button><Button variant="outline" className="cursor-pointer" disabled={busy || status.state === "disconnected"} onClick={() => void configure("disconnect")}><Unplug />Desconectar</Button></div>
        <div role="status" className="space-y-2 rounded-lg border p-3"><Badge variant={status.state === "connected" ? "default" : "secondary"}>{status.state === "connected" ? "Conectado" : status.state === "connecting" ? "Conectando" : status.state === "error" ? "Aguardando Jarvis" : "Desconectado"}</Badge><p className="text-sm text-muted-foreground">{status.message}</p></div>
      </CardContent>
    </Card>
    <p className="text-sm leading-relaxed text-muted-foreground">O Jarvis pode abrir novas abas e controlar as páginas que você conectar explicitamente. O navegador mostrará seu aviso de depuração durante o uso. A conexão é local neste computador; desconectar preserva suas abas.</p>
    <Toaster theme="dark" richColors />
  </main>;
}

createRoot(document.getElementById("root")!).render(<Options />);
