import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { LockKeyhole, Search } from "lucide-react";
import { z } from "zod";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { CardsSkeleton } from "@/components/layout/LoadingSkeletons";
import { libraryError } from "@/core/library";
import { CAPABILITY_LABELS, type CustomAgent } from "@/core/workflow-catalog";
import { ChoiceField } from "./WorkflowFields";

const permissions = z.array(z.object({ id: z.string(), name: z.string(), group: z.string(), description: z.string(), required: z.boolean(), capabilities: z.array(z.enum(["read_only", "write_files", "commands"])) }));

export function AgentToolPermissions({ agent, disabled, onChange }: { agent: CustomAgent; disabled: boolean; onChange: (patch: Partial<CustomAgent>) => void }) {
  const [tools, setTools] = useState<z.infer<typeof permissions>>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let alive = true;
    void invoke("get_agent_tool_permissions").then(value => { const parsed = permissions.parse(value); if (alive) setTools(parsed); }).catch(cause => { if (alive) setError(libraryError(cause, "Não foi possível carregar as ferramentas.")); }).finally(() => { if (alive) setLoading(false); });
    return () => { alive = false; };
  }, [attempt]);
  const visible = tools.filter(tool => `${tool.name} ${tool.description} ${tool.group}`.toLocaleLowerCase("pt-BR").includes(query.toLocaleLowerCase("pt-BR")));
  const groups = [...new Set(visible.map(tool => tool.group))];
  return <div className="space-y-5 p-5">
    <div className="grid gap-4 sm:grid-cols-2"><fieldset disabled={disabled}><ChoiceField label="Acesso ao projeto" value={agent.capability} onChange={value => onChange({ capability: value as CustomAgent["capability"] })} options={Object.entries(CAPABILITY_LABELS).map(([value, label]) => ({ value, label }))} /></fieldset><div className="space-y-2"><Label htmlFor="agent-tool-search">Ferramentas</Label><div className="relative"><Search className="absolute left-3 top-2.5 size-4 text-muted-foreground" /><Input id="agent-tool-search" className="pl-9" placeholder="Buscar ferramenta" value={query} onChange={event => setQuery(event.target.value)} /></div></div></div>
    <p className="text-xs text-muted-foreground">As permissões respeitam o acesso ao projeto e os serviços habilitados. A memória do Core permanece ativa.</p>
    {loading ? <CardsSkeleton label="Carregando permissões das ferramentas" columns /> : error ? <div className="space-y-3"><p role="alert" className="text-sm text-destructive">{error}</p><Button variant="outline" type="button" onClick={() => { setLoading(true); setError(null); setAttempt(n => n + 1); }}>Tentar novamente</Button></div> : groups.map(group => <section key={group} className="space-y-2"><h3 className="micro-label text-muted-foreground">{group}</h3><div className="grid gap-2 sm:grid-cols-2">{visible.filter(tool => tool.group === group).map(tool => {
      const capable = tool.capabilities.includes(agent.capability);
      const mcpDenied = tool.id !== "mcp_*" && tool.id.startsWith("mcp_") && agent.deniedTools?.includes("mcp_*");
      const checked = tool.required || (capable && !mcpDenied && !agent.deniedTools?.includes(tool.id));
      const locked = tool.required || !capable || Boolean(mcpDenied);
      return <div key={tool.id} className="flex min-w-0 items-start gap-3 rounded-lg border border-border bg-card p-3 shadow-(--edge-highlight)">
        <div className="min-w-0 flex-1">
          <Label htmlFor={`permission-${tool.id}`} className="cursor-pointer truncate font-mono text-xs" title={tool.name}>{tool.name}</Label>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{tool.description}</p>
          {locked && <Badge variant="outline" className="mt-2 gap-1 text-[10px] text-muted-foreground"><LockKeyhole className="size-3" />{tool.required ? "Obrigatória" : !tool.capabilities.length ? "Fluxos nativos" : !capable ? "Restrita pelo acesso" : "MCPs desativados"}</Badge>}
        </div>
        <Switch id={`permission-${tool.id}`} aria-label={tool.name} checked={checked} disabled={disabled || locked} className="mt-0.5 shrink-0 cursor-pointer" onCheckedChange={next => onChange({ deniedTools: next ? (agent.deniedTools ?? []).filter(id => id !== tool.id) : [...(agent.deniedTools ?? []), tool.id] })} />
      </div>;
    })}</div></section>)}
    {!loading && !error && !visible.length && <p className="text-sm text-muted-foreground">Nenhuma ferramenta encontrada.</p>}
  </div>;
}
