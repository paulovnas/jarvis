import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Globe } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldLabel } from "@/components/ui/field";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import type { ProviderAccount } from "@/core/provider-accounts";
import { webSearchConfigSchema } from "@/core/web-search";

const OFF = "off";

export function WebSearchSettings({ accounts }: { accounts: ProviderAccount[] }) {
  const [selected, setSelected] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [retry, setRetry] = useState(0);
  const savingRef = useRef(false);
  const mountedRef = useRef(false);

  useEffect(() => {
    let active = true;
    mountedRef.current = true;
    void invoke<unknown>("get_web_search_config").then((value) => {
      if (!active) return;
      const config = webSearchConfigSchema.parse(value);
      setSelected(config.accountAlias);
      setError(null);
    }).catch(() => {
      if (active) setError("Não foi possível carregar a configuração de Web Search.");
    }).finally(() => {
      if (active) setLoading(false);
    });
    return () => { active = false; mountedRef.current = false; };
  }, [retry]);

  const compatible = accounts.filter((account) => account.providerKind === "openai-codex");
  const unavailable = selected !== null && !compatible.some((account) => account.alias === selected);
  const items = [
    { value: OFF, label: "Desligado" },
    ...compatible.map((account) => ({ value: account.alias, label: account.alias })),
    ...(unavailable ? [{ value: selected, label: `${selected} · Indisponível` }] : []),
  ];

  async function save(value: string | null) {
    if (value === null || savingRef.current) return;
    const accountAlias = value === OFF ? null : value;
    if (accountAlias === selected) return;
    savingRef.current = true;
    setSaving(true);
    try {
      const config = webSearchConfigSchema.parse(await invoke<unknown>("set_web_search_config", { accountAlias }));
      if (mountedRef.current) setSelected(config.accountAlias);
      toast.success(config.accountAlias ? "Conta de Web Search atualizada" : "Web Search desligado");
    } catch {
      toast.error("Não foi possível salvar o Web Search. A seleção anterior foi mantida.");
    } finally {
      savingRef.current = false;
      if (mountedRef.current) setSaving(false);
    }
  }

  return <Card className="gap-4 py-4">
    <CardHeader className="gap-1 px-4">
      <CardTitle className="flex items-center gap-2 text-sm"><Globe aria-hidden="true" className="size-4 text-primary" />Web Search</CardTitle>
      <CardDescription className="text-xs">Escolha a conta usada nas pesquisas na web, independentemente da conta da conversa.</CardDescription>
    </CardHeader>
    <CardContent className="px-4">
      {loading ? <Skeleton className="h-8 w-full" role="status" aria-label="Carregando Web Search" /> : error ?
        <div className="flex flex-col gap-2"><p role="alert" className="text-xs text-destructive">{error}</p><Button variant="outline" size="sm" className="cursor-pointer self-start" onClick={() => { setLoading(true); setRetry((value) => value + 1); }}>Recarregar Web Search</Button></div> :
        <Field>
          <FieldLabel htmlFor="web-search-account" className="text-xs">Conta para pesquisa</FieldLabel>
          <Select items={items} value={selected ?? OFF} onValueChange={(value) => { void save(value); }} disabled={saving}>
            <SelectTrigger id="web-search-account" className="w-full cursor-pointer" aria-describedby="web-search-status"><SelectValue />{saving && <Spinner aria-hidden="true" />}</SelectTrigger>
            <SelectContent><SelectGroup>{items.map((item) => <SelectItem key={item.value} value={item.value} disabled={unavailable && item.value === selected} className="cursor-pointer">{item.label}</SelectItem>)}</SelectGroup></SelectContent>
          </Select>
          <p id="web-search-status" className="text-xs text-muted-foreground" role="status">{saving ? "Salvando…" : unavailable ? "A conta selecionada está indisponível. Escolha outra conta ou desligue a pesquisa." : compatible.length === 0 ? "Conecte uma conta compatível para habilitar a pesquisa." : selected ? "O agente poderá pesquisar e incluir fontes nas respostas." : "O agente não usará a ferramenta de pesquisa na web."}</p>
        </Field>}
    </CardContent>
  </Card>;
}
