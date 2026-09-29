import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { FileUp, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input, Textarea } from "@/components/TextInput";
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Checkbox } from "@/components/ui/checkbox";
import { Hint } from "@/components/ui/hint";
import { httpFileSchema, type HttpRequest } from "@/core/http-client";
import { libraryError } from "@/core/library";
import { HttpPairsEditor } from "./HttpPairsEditor";

const authLabels = { none: "Sem autenticação", basic: "Basic", bearer: "Bearer token", apiKey: "API key" } as const;
const bodyLabels = { none: "Sem corpo", json: "JSON", text: "Texto", urlencoded: "Formulário URL-encoded", multipart: "Multipart", binary: "Arquivo binário" } as const;

export function HttpRequestEditor({ projectId, request, onChange, disabled }: { projectId: string; request: HttpRequest; onChange: (request: HttpRequest) => void; disabled: boolean }) {
  const [files, setFiles] = useState<Record<string, string>>({});
  const [uploading, setUploading] = useState(false);
  const locked = disabled || uploading;
  const patch = (next: Partial<HttpRequest>) => onChange({ ...request, ...next });
  const body = request.body;
  const auth = request.auth;
  const file = async (index?: number) => {
    setUploading(true);
    try {
      const path = await open({ multiple: false, directory: false });
      if (typeof path !== "string") return;
      const stored = httpFileSchema.parse(await invoke("import_http_file", { projectId, path }));
      setFiles(current => ({ ...current, [stored.id]: stored.name }));
      patch({ body: index === undefined ? { ...body, fileId: stored.id } : { ...body, fields: body.fields.map((item, i) => i === index ? { ...item, fileId: stored.id, value: "" } : item) } });
    } catch (cause) { toast.error(libraryError(cause, "Não foi possível importar o arquivo.")); } finally { setUploading(false); }
  };
  return <Tabs defaultValue="params" className="min-w-0 gap-3">
    <TabsList className="h-8 w-fit max-w-full overflow-x-auto"><TabsTrigger value="params" className="cursor-pointer text-xs">Params {request.params.filter(pair => pair.enabled).length || ""}</TabsTrigger><TabsTrigger value="headers" className="cursor-pointer text-xs">Headers {request.headers.filter(pair => pair.enabled).length || ""}</TabsTrigger><TabsTrigger value="auth" className="cursor-pointer text-xs">Autenticação</TabsTrigger><TabsTrigger value="body" className="cursor-pointer text-xs">Corpo</TabsTrigger></TabsList>
    <TabsContent value="params"><HttpPairsEditor label="parâmetro" pairs={request.params} disabled={locked} onChange={params => patch({ params })} /></TabsContent>
    <TabsContent value="headers"><HttpPairsEditor label="header" pairs={request.headers} disabled={locked} onChange={headers => patch({ headers })} /></TabsContent>
    <TabsContent value="auth"><FieldGroup className="gap-3"><Field><FieldLabel>Tipo de autenticação</FieldLabel><Select value={auth.type} disabled={locked} onValueChange={type => { if (type && type in authLabels) patch({ auth: { ...auth, type: type as typeof auth.type } }); }}><SelectTrigger aria-label="Tipo de autenticação" className="w-full cursor-pointer sm:w-64"><SelectValue>{authLabels[auth.type]}</SelectValue></SelectTrigger><SelectContent><SelectGroup>{Object.entries(authLabels).map(([value, label]) => <SelectItem key={value} value={value} className="cursor-pointer">{label}</SelectItem>)}</SelectGroup></SelectContent></Select><FieldDescription>Use referências como {"{{token}}"} para segredos cadastrados no projeto.</FieldDescription></Field>
      {auth.type === "basic" && <><Field><FieldLabel htmlFor="http-auth-user">Usuário</FieldLabel><Input id="http-auth-user" value={auth.username} disabled={locked} onChange={event => patch({ auth: { ...auth, username: event.target.value } })} /></Field><Field><FieldLabel htmlFor="http-auth-password">Senha ou variável</FieldLabel><Input id="http-auth-password" type="password" autoComplete="off" value={auth.password} disabled={locked} onChange={event => patch({ auth: { ...auth, password: event.target.value } })} /></Field></>}
      {auth.type === "bearer" && <Field><FieldLabel htmlFor="http-auth-token">Token ou variável</FieldLabel><Input id="http-auth-token" type="password" autoComplete="off" value={auth.token} disabled={locked} onChange={event => patch({ auth: { ...auth, token: event.target.value } })} /></Field>}
      {auth.type === "apiKey" && <><Field><FieldLabel htmlFor="http-auth-name">Nome da chave</FieldLabel><Input id="http-auth-name" value={auth.name} disabled={locked} onChange={event => patch({ auth: { ...auth, name: event.target.value } })} /></Field><Field><FieldLabel htmlFor="http-auth-value">Valor ou variável</FieldLabel><Input id="http-auth-value" type="password" autoComplete="off" value={auth.value} disabled={locked} onChange={event => patch({ auth: { ...auth, value: event.target.value } })} /></Field><Field><FieldLabel>Enviar em</FieldLabel><Select value={auth.location} disabled={locked} onValueChange={location => { if (location === "header" || location === "query") patch({ auth: { ...auth, location } }); }}><SelectTrigger aria-label="Local da API key" className="w-44 cursor-pointer"><SelectValue>{auth.location === "header" ? "Header" : "Query string"}</SelectValue></SelectTrigger><SelectContent><SelectGroup><SelectItem value="header" className="cursor-pointer">Header</SelectItem><SelectItem value="query" className="cursor-pointer">Query string</SelectItem></SelectGroup></SelectContent></Select></Field></>}
    </FieldGroup></TabsContent>
    <TabsContent value="body"><FieldGroup className="gap-3"><Field><FieldLabel>Formato do corpo</FieldLabel><Select value={body.type} disabled={locked} onValueChange={type => { if (type && type in bodyLabels) patch({ body: { ...body, type: type as typeof body.type } }); }}><SelectTrigger aria-label="Formato do corpo" className="w-full cursor-pointer sm:w-64"><SelectValue>{bodyLabels[body.type]}</SelectValue></SelectTrigger><SelectContent><SelectGroup>{Object.entries(bodyLabels).map(([value, label]) => <SelectItem key={value} value={value} className="cursor-pointer">{label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
      {(body.type === "json" || body.type === "text") && <Field><FieldLabel htmlFor="http-body-text">Conteúdo</FieldLabel><Textarea id="http-body-text" value={body.text} disabled={locked} spellCheck={false} className="min-h-36 resize-y font-mono text-xs" onChange={event => patch({ body: { ...body, text: event.target.value } })} /></Field>}
      {body.type === "urlencoded" && <HttpPairsEditor label="campo" pairs={body.fields} disabled={locked} onChange={fields => patch({ body: { ...body, fields: fields.map(field => ({ ...field, fileId: null })) } })} />}
      {body.type === "multipart" && <div className="flex flex-col gap-2">{body.fields.map((item, index) => <div key={index} className="flex flex-wrap items-center gap-2"><Checkbox aria-label={`Ativar campo multipart ${index + 1}`} checked={item.enabled} disabled={locked} className="cursor-pointer" onCheckedChange={enabled => patch({ body: { ...body, fields: body.fields.map((field, i) => i === index ? { ...field, enabled: enabled === true } : field) } })} /><Input aria-label={`Nome multipart ${index + 1}`} placeholder="Nome" value={item.name} disabled={locked} className="min-w-20 flex-1 font-mono text-xs" onChange={event => patch({ body: { ...body, fields: body.fields.map((field, i) => i === index ? { ...field, name: event.target.value } : field) } })} />{item.fileId ? <span className="min-w-0 flex-1 truncate font-mono text-xs">{files[item.fileId] ?? `Arquivo ${item.fileId}`}</span> : <Input aria-label={`Valor multipart ${index + 1}`} placeholder="Valor" value={item.value} disabled={locked} className="min-w-20 flex-1 font-mono text-xs" onChange={event => patch({ body: { ...body, fields: body.fields.map((field, i) => i === index ? { ...field, value: event.target.value } : field) } })} />}<Hint content="Escolher arquivo"><Button variant="outline" size="icon-sm" aria-label={`Arquivo multipart ${index + 1}`} disabled={locked} className="cursor-pointer" onClick={() => void file(index)}><FileUp /></Button></Hint><Hint content="Remover campo"><Button variant="ghost" size="icon-sm" aria-label={`Remover multipart ${index + 1}`} disabled={locked} className="cursor-pointer" onClick={() => patch({ body: { ...body, fields: body.fields.filter((_, i) => i !== index) } })}><Trash2 /></Button></Hint></div>)}<Button variant="outline" size="sm" disabled={locked} className="w-fit cursor-pointer" onClick={() => patch({ body: { ...body, fields: [...body.fields, { name: "", value: "", enabled: true, fileId: null }] } })}>Adicionar campo</Button></div>}
      {body.type === "binary" && <Field><FieldLabel>Arquivo</FieldLabel><Button variant="outline" disabled={locked} className="w-fit max-w-full cursor-pointer" onClick={() => void file()}><FileUp /><span className="truncate">{body.fileId ? files[body.fileId] ?? `Arquivo ${body.fileId}` : "Selecionar arquivo"}</span></Button></Field>}
    </FieldGroup></TabsContent>
  </Tabs>;
}
