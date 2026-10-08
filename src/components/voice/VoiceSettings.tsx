import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, Download, Mic, RefreshCw, ShieldCheck, Volume2, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { Progress } from "@/components/ui/progress";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Skeleton } from "@/components/ui/skeleton";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useVoice } from "@/hooks/use-voice";
import { voicePhaseLabels, type VoiceConfig } from "@/core/voice";

const voices = { pm_alex: "Alex · masculino", pf_dora: "Dora · feminino", pm_santa: "Santa · masculino" };
const message = (cause: unknown) => typeof cause === "string" ? cause : "Não foi possível preparar o Jarvis Voice.";

export function VoiceSettings({ compact = false }: { compact?: boolean }) {
  const voice = useVoice();
  const [busy, setBusy] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const settings = voice.settings;
  const config = settings?.config;
  const save = async (patch: Partial<VoiceConfig>) => {
    if (!config || busy) return;
    setBusy(true); setError(null);
    try { await voice.save({ ...config, ...patch }); }
    catch (cause) { setError(message(cause)); }
    finally { setBusy(false); }
  };
  const install = async () => {
    if (!config || installing) return;
    setInstalling(true); setError(null);
    try { await invoke("install_voice_model", { model: config.model }); await voice.refresh(); toast.success("Transcrição local pronta"); }
    catch (cause) { setError(message(cause)); }
    finally { setInstalling(false); await voice.refresh(); }
  };
  if (!settings || !config) return <div className="space-y-3">{voice.error ? <Alert><AlertDescription>{voice.error}<Button variant="outline" size="sm" className="mt-2 cursor-pointer" onClick={() => { void voice.refresh(); }}>Tentar novamente</Button></AlertDescription></Alert> : <div role="status" aria-label="Carregando configurações de voz"><Skeleton className="h-20" /><Skeleton className="mt-3 h-36" /></div>}</div>;
  const installed = settings.models.find(model => model.id === config.model)?.installed;
  const download = settings.download;
  return <div className="space-y-4">
    <Card><CardHeader className="flex flex-row items-center gap-3"><div className="min-w-0 flex-1"><CardTitle className="flex items-center gap-2"><Mic className="size-4 text-primary" />Jarvis Voice</CardTitle><CardDescription className="mt-1">Ditado no chat e avisos falados do Jarvito.</CardDescription></div><Switch aria-label="Ativar Jarvis Voice" checked={config.enabled} disabled={busy} className="cursor-pointer" onCheckedChange={enabled => { void save({ enabled }); }} /></CardHeader>
      <CardContent className="space-y-4">
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="space-y-1.5"><Label htmlFor="voice-mic">Microfone</Label><Select value={config.microphone ?? "default"} onValueChange={id => { if (id) void save({ microphone: id === "default" ? null : id }); }} disabled={busy || voice.active}><SelectTrigger id="voice-mic" className="w-full min-w-0 cursor-pointer"><SelectValue>{settings.microphones.find(device => device.id === config.microphone)?.name ?? "Padrão do sistema"}</SelectValue></SelectTrigger><SelectContent><SelectGroup><SelectItem value="default" className="cursor-pointer">Padrão do sistema</SelectItem>{settings.microphones.map(device => <SelectItem key={device.id} value={device.id} className="cursor-pointer">{device.name}</SelectItem>)}</SelectGroup></SelectContent></Select></div>
          <div className="space-y-1.5"><Label htmlFor="voice-speaker">Saída de áudio</Label><Select value={config.speaker ?? "default"} onValueChange={id => { if (id) void save({ speaker: id === "default" ? null : id }); }} disabled={busy || voice.active}><SelectTrigger id="voice-speaker" className="w-full min-w-0 cursor-pointer"><SelectValue>{settings.speakers.find(device => device.id === config.speaker)?.name ?? "Padrão do sistema"}</SelectValue></SelectTrigger><SelectContent><SelectGroup><SelectItem value="default" className="cursor-pointer">Padrão do sistema</SelectItem>{settings.speakers.map(device => <SelectItem key={device.id} value={device.id} className="cursor-pointer">{device.name}</SelectItem>)}</SelectGroup></SelectContent></Select></div>
        </div>
        <div className="space-y-1.5"><Label htmlFor="voice-model">Transcrição local · português</Label><Select value={config.model} onValueChange={model => { if (model === "small" || model === "tiny") void save({ model }); }} disabled={busy || installing || !!download || voice.active}><SelectTrigger id="voice-model" className="w-full cursor-pointer"><SelectValue>{settings.models.find(model => model.id === config.model)?.name}</SelectValue></SelectTrigger><SelectContent><SelectGroup>{settings.models.map(model => <SelectItem key={model.id} value={model.id} className="cursor-pointer">{model.name} · {Math.ceil(model.bytes / 1e6)} MB</SelectItem>)}</SelectGroup></SelectContent></Select>
          <div className="flex flex-wrap items-center gap-2">{installed ? <span className="flex items-center gap-1 text-xs text-onedark-green"><Check className="size-3" />Modelo pronto</span> : <Button size="sm" variant="outline" className="cursor-pointer" disabled={installing || !!download} onClick={() => { void install(); }}><Download className="size-3.5" />{installing || download ? "Preparando…" : "Baixar modelo"}</Button>}<Button size="sm" variant="ghost" className="cursor-pointer text-muted-foreground" onClick={() => { void voice.refresh(); }}><RefreshCw className="size-3" />Dispositivos</Button></div>
          {(download || installing) && <div role="status" aria-label="Baixando modelo de voz" className="space-y-1"><Progress value={download ? download.received / download.total * 100 : 0} /><div className="flex items-center justify-between gap-2"><span className="font-mono text-xs text-muted-foreground">{download ? `${(download.received / 1e6).toFixed(1)} / ${Math.ceil(download.total / 1e6)} MB` : "Conectando…"}</span><Button size="sm" variant="ghost" className="cursor-pointer" onClick={() => { void invoke("cancel_voice_download").catch(cause => setError(message(cause))); }}><X className="size-3" />Cancelar</Button></div></div>}
        </div>
        <div className="grid gap-3 sm:grid-cols-2"><div className="space-y-1.5"><Label htmlFor="voice-name">Voz em português</Label><Select value={config.voice} onValueChange={value => { if (value && value in voices) void save({ voice: value as VoiceConfig["voice"] }); }} disabled={busy || voice.active}><SelectTrigger id="voice-name" className="w-full cursor-pointer"><SelectValue>{voices[config.voice]}</SelectValue></SelectTrigger><SelectContent><SelectGroup>{Object.entries(voices).map(([id, name]) => <SelectItem value={id} key={id} className="cursor-pointer">{name}</SelectItem>)}</SelectGroup></SelectContent></Select></div><div className="space-y-2"><Label id="voice-speed-label" htmlFor="voice-speed">Velocidade · {config.speed.toFixed(2)}×</Label><Slider id="voice-speed" aria-labelledby="voice-speed-label" value={[config.speed]} min={0.75} max={1.5} step={0.05} disabled={busy || voice.active} onValueCommitted={values => { const speed = Array.isArray(values) ? values[0] : values; if (typeof speed === "number") void save({ speed }); }} className="mt-4 cursor-pointer" /></div></div>
        {!compact && <div className="space-y-2"><Label id="voice-silence-label" htmlFor="voice-silence">Pausa para enviar a fala · {(config.silenceMs / 1000).toFixed(2)} s</Label><Slider id="voice-silence" aria-labelledby="voice-silence-label" value={[config.silenceMs]} min={400} max={1800} step={50} disabled={busy || voice.active} onValueCommitted={values => { const silenceMs = Array.isArray(values) ? values[0] : values; if (typeof silenceMs === "number") void save({ silenceMs }); }} className="cursor-pointer" /></div>}
        <Button variant="outline" className="cursor-pointer" disabled={!config.enabled || !settings.speechReady || busy || voice.active} onClick={() => { void voice.start("voice-test", "test").catch(cause => setError(message(cause))); }}><Volume2 className="size-4" />Testar voz</Button>
        {voice.active && voice.session?.mode === "test" && <div className="flex items-center gap-2"><span role="status" className="text-xs text-primary">{voicePhaseLabels[voice.session.phase]}</span><Button size="sm" variant="ghost" className="cursor-pointer" onClick={() => { if (voice.session?.id) void voice.control(voice.session.id, "end").catch(cause => setError(message(cause))); }}>Parar teste</Button></div>}
        {!settings.speechReady && <Alert><AlertDescription>O teste da voz local requer o recurso de voz. Os áudios de notificação e o ditado continuam disponíveis; o ditado funciona com o modelo de transcrição.</AlertDescription></Alert>}
        {(error || voice.session?.phase === "error") && <p role="alert" className="text-sm text-destructive">{error || voice.session?.error}</p>}
      </CardContent>
    </Card>
    <p className="flex items-start gap-2 text-xs leading-5 text-muted-foreground"><ShieldCheck className="mt-1 size-3.5 shrink-0 text-onedark-green" />O microfone só abre ao iniciar um ditado. Transcrição e voz são locais; apenas o texto segue para o modelo da conversa. Áudio não é salvo no histórico.</p>
  </div>;
}

export function VoiceSettingsDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  return <Dialog open={open} onOpenChange={onOpenChange}><DialogContent className="max-h-[85dvh] max-w-xl overflow-y-auto"><DialogHeader><DialogTitle>Jarvis Voice</DialogTitle><DialogDescription>Prepare a voz local e seus dispositivos.</DialogDescription></DialogHeader>{open && <VoiceSettings compact />}</DialogContent></Dialog>;
}
