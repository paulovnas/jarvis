import { memo, useCallback, useMemo, useState } from "react";
import { applyNodeChanges, Background, Handle, Panel, Position, ReactFlow, ReactFlowProvider, useReactFlow, type Edge, type Node, type NodeProps, type OnNodesChange } from "@xyflow/react";
import { Flag, GripVertical, LockKeyhole, Maximize, Minus, Plus } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { isBuiltinAgent, type CustomFlow, type FlowAgent, type WorkflowGraph } from "@/core/workflow-catalog";
import type { WorkflowAppearance } from "@/core/workflow-appearance";
import { WorkflowIdentityIcon } from "@/components/agents/WorkflowIdentityIcon";
import "@xyflow/react/dist/style.css";
import "./workflow-canvas.css";

type AgentNode = Node<{ label: string; description: string; entry: boolean; builtin: boolean; delegation: boolean; appearance?: WorkflowAppearance | null }, "agent">;
const AgentBlock = memo(function AgentBlock({ data, selected }: NodeProps<AgentNode>) {
  return <Card className={`workflow-node w-56 cursor-pointer gap-0 py-0 ${selected ? "border-primary ring-1 ring-primary/30" : "border-border"}`}>
    <Handle type="target" position={Position.Left} className="cursor-pointer" aria-label="Entrada da etapa" />
    <CardContent className="p-3"><div className="flex items-center gap-2"><WorkflowIdentityIcon appearance={data.appearance} className="size-4 shrink-0" /><span className="min-w-0 flex-1 truncate text-xs font-medium">{data.label}</span><GripVertical className="size-3.5 text-muted-foreground" /></div>
      <div className="mt-2 flex flex-wrap gap-1.5">{data.entry && <Badge variant="outline" className="gap-1 border-primary/40 text-[10px] text-primary"><Flag className="size-2.5" />Início</Badge>}{data.builtin && <Badge variant="outline" className="gap-1 text-[10px] text-muted-foreground"><LockKeyhole className="size-2.5" />Jarvis</Badge>}</div>
      <p className="mt-2 line-clamp-2 min-h-8 text-[11px] leading-4 text-muted-foreground">{data.description || "Execute as instruções deste agente."}</p>
      <div className="mt-3 flex justify-between border-t border-border pt-2 font-mono text-[9px]">{data.delegation ? <span className="text-onedark-cyan">Delega conforme o escopo →</span> : <><span className="text-primary">Concluído →</span><span className="text-destructive">Correção ↓</span></>}</div>
    </CardContent>
    {data.delegation ? <Handle type="source" position={Position.Right} id="delegation" className="cursor-pointer !bg-onedark-cyan" aria-label="Saída de delegação" /> : <><Handle type="source" position={Position.Right} id="next" className="cursor-pointer !bg-primary" aria-label="Saída de conclusão" /><Handle type="source" position={Position.Bottom} id="onRework" className="cursor-pointer !bg-destructive" aria-label="Saída de correção" /></>}
  </Card>;
});
const nodeTypes = { agent: AgentBlock };
function reconcileNodes(previous: AgentNode[], flow: WorkflowGraph, agents: FlowAgent[], selected: string | null): AgentNode[] {
  const delegation = "connections" in flow;
  return flow.steps.map(step => {
    const old = previous.find(node => node.id === step.id);
    const agent = agents.find(item => item.id === step.agentId);
    const data = { label: agent?.name ?? "Agente indisponível", description: step.instructions || agent?.description || "", entry: step.id === flow.entry, builtin: agent ? isBuiltinAgent(agent) : false, delegation, appearance: agent?.appearance };
    const sameData = old && old.data.label === data.label && old.data.description === data.description && old.data.entry === data.entry && old.data.builtin === data.builtin && old.data.delegation === data.delegation && old.data.appearance === data.appearance;
    // Keep measured dimensions and drag state: dropping them hides nodes while React Flow remeasures.
    return { ...old, id: step.id, type: "agent", position: step.position, selected: step.id === selected, data: sameData ? old.data : data };
  });
}
function CanvasControls() {
  const { zoomIn, zoomOut, fitView } = useReactFlow();
  return <Panel position="bottom-left" className="flex gap-1 rounded-md border border-border bg-card p-1"><Button type="button" variant="ghost" size="icon" className="size-7 cursor-pointer" aria-label="Aumentar zoom" onClick={() => void zoomIn()}><Plus /></Button><Button type="button" variant="ghost" size="icon" className="size-7 cursor-pointer" aria-label="Diminuir zoom" onClick={() => void zoomOut()}><Minus /></Button><Button type="button" variant="ghost" size="icon" className="size-7 cursor-pointer" aria-label="Enquadrar fluxo" onClick={() => void fitView({ padding: 0.2 })}><Maximize /></Button></Panel>;
}
export default function WorkflowCanvas({ flow, agents, selected, onSelect, onChange, disabled }: { flow: WorkflowGraph; agents: FlowAgent[]; selected: string | null; onSelect: (id: string | null) => void; onChange: (flow: CustomFlow) => void; disabled: boolean }) {
  const [state, setState] = useState(() => ({ flow, agents, selected, nodes: reconcileNodes([], flow, agents, selected) }));
  if (state.flow !== flow || state.agents !== agents || state.selected !== selected) {
    setState({ flow, agents, selected, nodes: reconcileNodes(state.nodes, flow, agents, selected) });
  }
  const onNodesChange = useCallback<OnNodesChange<AgentNode>>(changes => {
    const accepted = disabled ? changes.filter(change => change.type === "dimensions") : changes;
    setState(current => ({ ...current, nodes: applyNodeChanges(accepted, current.nodes) }));
    if (disabled) return;
    const selection = changes.find(change => change.type === "select" && change.selected);
    if (selection?.type === "select") onSelect(selection.id);
    const positions = changes.filter(change => change.type === "position");
    if (!("connections" in flow) && positions.some(change => change.position)) onChange({ ...flow, steps: flow.steps.map(step => { const change = positions.find(item => item.id === step.id); return change?.position ? { ...step, position: change.position } : step; }) });
  }, [disabled, flow, onChange, onSelect]);
  const edges = useMemo<Edge[]>(() => "connections" in flow ? flow.connections.map(connection => ({ id: connection.id, source: connection.source, sourceHandle: "delegation", target: connection.target, label: connection.label, type: "smoothstep", style: { stroke: "var(--color-onedark-cyan)" }, labelStyle: { fill: "var(--muted-foreground)", fontSize: 10 }, labelBgStyle: { fill: "var(--card)" } })) : flow.steps.flatMap(step => (["next", "onRework"] as const).flatMap(kind => step[kind] ? [{ id: `${step.id}/${kind}`, source: step.id, sourceHandle: kind, target: step[kind], label: kind === "next" ? "Concluído" : "Correção", type: "smoothstep", style: { stroke: kind === "next" ? "var(--primary)" : "var(--destructive)" }, labelStyle: { fill: "var(--muted-foreground)", fontSize: 10 }, labelBgStyle: { fill: "var(--card)" } }] : [])), [flow]);
  return <div className="workflow-canvas h-full min-h-64 overflow-hidden rounded-md border border-border bg-sidebar" aria-label="Canvas do fluxo"><ReactFlowProvider><ReactFlow<AgentNode>
    nodes={state.nodes} edges={edges} nodeTypes={nodeTypes} colorMode="dark" fitView minZoom={0.2} maxZoom={1.6}
    nodesDraggable={!disabled} nodesConnectable={!disabled} edgesReconnectable={false} deleteKeyCode={null}
    onNodeClick={(_event, node) => onSelect(node.id)} onPaneClick={() => onSelect(null)}
    onNodesChange={onNodesChange}
    onConnect={connection => {
      if (disabled || !connection.source || !connection.target || !["next", "onRework"].includes(connection.sourceHandle ?? "")) return;
      if (!("connections" in flow)) onChange({ ...flow, steps: flow.steps.map(step => step.id === connection.source ? { ...step, [connection.sourceHandle === "next" ? "next" : "onRework"]: connection.target } : step) });
    }}
    ariaLabelConfig={{ "node.a11yDescription.default": "Pressione Enter para selecionar um bloco. Use as setas para movê-lo.", "controls.zoomIn.ariaLabel": "Aumentar zoom", "controls.zoomOut.ariaLabel": "Diminuir zoom", "controls.fitView.ariaLabel": "Enquadrar fluxo" }}
  ><Background color="var(--border)" gap={20} /><CanvasControls /></ReactFlow></ReactFlowProvider></div>;
}
