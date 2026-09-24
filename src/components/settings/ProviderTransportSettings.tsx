import { useEffect, useId, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import { toast } from "sonner";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";

const settingsSchema = z.object({ supported: z.boolean(), enabled: z.boolean() });

export function ProviderTransportSettings({ alias }: { alias: string }) {
  return <AccountTransportSettings key={alias} alias={alias} />;
}

function AccountTransportSettings({ alias }: { alias: string }) {
  const id = useId();
  const [settings, setSettings] = useState<z.infer<typeof settingsSchema> | null>(null);
  const [failed, setFailed] = useState(false);
  const [saving, setSaving] = useState(false);
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    let active = true;
    void invoke("get_provider_transport", { alias })
      .then(value => {
        const settings = settingsSchema.parse(value);
        if (active) {
          setSettings(settings);
          setFailed(false);
        }
      })
      .catch(() => { if (active) setFailed(true); });
    return () => { active = false; };
  }, [alias, revision]);
  if (failed) return (
    <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
      <p>Não foi possível consultar a conexão incremental.</p>
      <Button size="sm" variant="ghost" className="cursor-pointer" onClick={() => { setFailed(false); setRevision(value => value + 1); }}>
        Tentar novamente
      </Button>
    </div>
  );
  if (!settings) return (
    <div role="status" aria-label="Carregando opção de conexão"><Skeleton className="h-10 w-full" /></div>
  );
  if (!settings.supported) return null;
  const save = async (enabled: boolean) => {
    setSaving(true);
    try {
      setSettings(settingsSchema.parse(await invoke("set_provider_transport", { alias, enabled })));
      toast.success("Preferência de conexão salva", { description: "Será aplicada nas próximas execuções desta conta." });
    } catch {
      toast.error("Não foi possível salvar a preferência de conexão");
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="flex items-start justify-between gap-4 border-t border-border pt-3">
      <div className="space-y-1">
        <Label htmlFor={id} className="cursor-pointer text-xs">Conexão incremental · experimental</Label>
        <p id={`${id}-description`} className="text-xs leading-5 text-muted-foreground">
          Reaproveita a conexão entre etapas para reduzir o envio do histórico. Se o provedor não aceitar, usa a conexão padrão. Vale para novas execuções.
        </p>
      </div>
      <Switch id={id} aria-describedby={`${id}-description`} checked={settings.enabled} disabled={saving} onCheckedChange={enabled => void save(enabled)} className="mt-0.5 cursor-pointer" />
    </div>
  );
}
