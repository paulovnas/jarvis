import { useState, type FormEvent, type ReactNode } from "react";
import { Activity, ArrowUp, ChevronDown, CircleStop } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Textarea } from "@/components/ui/textarea";
import type { currentActivity } from "./remote-activity";

export function RemoteComposer({ value, onChange, onSubmit, onCancel, working, busy, sendDisabled = false, model, activity, selectors, selectionError }: {
  value: string; onChange: (value: string) => void; onSubmit: () => void; onCancel?: () => void;
  working: boolean; busy: boolean; model: string; activity: ReturnType<typeof currentActivity> | null;
  sendDisabled?: boolean; selectors?: ReactNode; selectionError?: string | null;
}) {
  const [focused, setFocused] = useState(false);
  const expanded = !working || focused || !!value;
  const submit = (event: FormEvent) => { event.preventDefault(); if (!busy && !sendDisabled && value.trim()) onSubmit(); };
  return <>
    {selectionError && <p role="alert" className="px-1 py-2 text-xs text-destructive">{selectionError}</p>}
    {working && activity && <Collapsible className="remote-progress" aria-label="Atividade atual" data-status={activity.status}>
      <CollapsibleTrigger render={<Button variant="ghost" className="remote-progress-trigger cursor-pointer" aria-label="Detalhes da atividade atual" />}>
        <Activity aria-hidden="true" /><span className="remote-progress-agent">{activity.agent ?? "Jarvis"}</span><span className="remote-progress-detail">{activity.detail ?? activity.tool ?? activity.label}</span><ChevronDown aria-hidden="true" />
      </CollapsibleTrigger>
      <CollapsibleContent className="remote-progress-content"><p>{activity.tool && <span className="font-mono">{activity.tool} · </span>}{activity.detail ?? activity.label}</p></CollapsibleContent>
    </Collapsible>}
    <form aria-label="Enviar mensagem ao Jarvis" className="chat-composer remote-composer" data-working={working} data-expanded={expanded} onSubmit={submit}>
      <FieldGroup className="remote-composer-field"><Field><FieldLabel className="sr-only" htmlFor="remote-message">Mensagem</FieldLabel><Textarea id="remote-message" rows={expanded ? 2 : 1} placeholder={working ? "Mensagem…" : "Mensagem para o Jarvis…"} maxLength={64000} value={value} onFocus={() => setFocused(true)} onBlur={() => setFocused(false)} onChange={event => onChange(event.target.value)} /></Field></FieldGroup>
      <div className="remote-composer-actions">{selectors ? <div hidden={!expanded} className="remote-composer-selectors">{selectors}</div> : <p hidden={!expanded} className="remote-composer-model">{model}</p>}{onCancel && <Button type="button" variant="ghost" size="icon" aria-label="Interromper execução" className="remote-stop cursor-pointer" disabled={busy} onClick={onCancel}><CircleStop /></Button>}<Button type="submit" size="icon" aria-label={working ? "Enviar mensagem para a fila" : "Enviar mensagem"} className="cursor-pointer" disabled={busy || sendDisabled || !value.trim()}><ArrowUp /></Button></div>
    </form>
  </>;
}
