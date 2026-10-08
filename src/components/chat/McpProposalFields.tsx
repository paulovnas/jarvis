import { useId } from "react";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import type { McpAuthoringValues, McpProposalServer } from "@/core/authoring";

export function McpProposalFields({ server, values, onChange, disabled }: {
  server: McpProposalServer; values: McpAuthoringValues; disabled: boolean;
  onChange: (group: keyof McpAuthoringValues, key: string, value: string) => void;
}) {
  const id = useId();
  const groups = [{ group: "environment", keys: server.envKeys, label: "Variável" }, { group: "headers", keys: server.headerKeys, label: "Cabeçalho" }] as const;
  return <Card>
    <CardHeader>
      <div className="flex flex-wrap items-center gap-2"><CardTitle className="break-all">{server.name}</CardTitle><Badge variant="outline">{server.enabled ? "Ativado" : "Desativado"}</Badge></div>
      <CardDescription>{server.transport === "stdio" ? "Local · stdio" : "Remoto · HTTP"}</CardDescription>
    </CardHeader>
    <CardContent className="flex min-w-0 flex-col gap-4">
      <dl className="flex min-w-0 flex-col gap-3 text-xs">
        <div><dt className="micro-label mb-1 text-muted-foreground">Escopo</dt><dd>Global · disponível nos projetos e chats do Jarvis</dd></div>
        {server.transport === "stdio" ? <>
          <div><dt className="micro-label mb-1 text-muted-foreground">Programa</dt><dd className="whitespace-pre-wrap break-all font-mono">{server.command}</dd></div>
          <div><dt className="micro-label mb-1 text-muted-foreground">Argumentos (na ordem)</dt><dd className="whitespace-pre-wrap break-all font-mono">{JSON.stringify(server.args)}</dd></div>
          <div><dt className="micro-label mb-1 text-muted-foreground">Pasta de trabalho</dt><dd className="whitespace-pre-wrap break-all font-mono">{server.cwd ?? "Não definida"}</dd></div>
        </> : <div><dt className="micro-label mb-1 text-muted-foreground">Endereço do servidor</dt><dd className="whitespace-pre-wrap break-all font-mono">{server.url}</dd></div>}
        {server.envKeys.length > 0 && <div><dt className="micro-label mb-2 text-muted-foreground">Variáveis de ambiente (nomes)</dt><dd className="flex flex-wrap gap-1.5">{server.envKeys.map(key => <Badge key={key} variant="outline" className="max-w-full whitespace-normal break-all font-mono">{key}</Badge>)}</dd></div>}
        {server.headerKeys.length > 0 && <div><dt className="micro-label mb-2 text-muted-foreground">Cabeçalhos (nomes)</dt><dd className="flex flex-wrap gap-1.5">{server.headerKeys.map(key => <Badge key={key} variant="outline" className="max-w-full whitespace-normal break-all font-mono">{key}</Badge>)}</dd></div>}
      </dl>
      <p className="text-xs leading-5 text-muted-foreground">{server.enabled ? `O Jarvis poderá ${server.transport === "stdio" ? "iniciar o processo" : "conectar ao servidor"} e descobrir suas ferramentas somente após sua aprovação.` : "O servidor será salvo e permanecerá desativado, sem iniciar processos ou conexões. Você poderá ativá-lo nas configurações."}</p>
      {(server.envKeys.length > 0 || server.headerKeys.length > 0) && <FieldSet>
        <FieldLegend>Valores para configurar o servidor</FieldLegend>
        <FieldDescription>Preencha os valores diretamente aqui. Eles serão enviados somente ao aprovar; não serão incluídos na proposta nem enviados à IA.</FieldDescription>
        <FieldGroup>{groups.flatMap(({ group, keys, label }) => keys.map((key, index) => <Field key={`${group}-${key}`} data-disabled={disabled}>
          <FieldLabel htmlFor={`${id}-${group}-${index}`} className="cursor-pointer break-all">{label} {key}</FieldLabel>
          <Input id={`${id}-${group}-${index}`} type="password" required autoComplete="new-password" spellCheck={false} value={Object.prototype.hasOwnProperty.call(values[group], key) ? values[group][key] : ""} disabled={disabled} onChange={event => onChange(group, key, event.target.value)} />
        </Field>))}</FieldGroup>
      </FieldSet>}
    </CardContent>
  </Card>;
}
