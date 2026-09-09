import type { ToolCallItem } from "./types";

const GROUP_THRESHOLD = 5;
const GROUP_SIZE = 16;

const actionLabels = {
  context: "usou Context Mode",
  filesRead: "leu e pesquisou arquivos",
  filesWrite: "alterou arquivos",
  terminal: "executou comandos",
  research: "consultou fontes externas",
  skills: "consultou skills",
  agents: "coordenou agentes",
  tasks: "organizou tarefas",
  design: "consultou recursos de design",
  media: "processou imagens e anexos",
  jarvis: "configurou o Jarvis",
  integrations: "usou integrações",
  other: "executou outras ações",
} as const;

type ActionKind = keyof typeof actionLabels;

export interface ToolActivityGroup {
  id: string;
  tools: ToolCallItem[];
  summary: string;
  failures: number;
  active: boolean;
}

function actionKind(name: string): ActionKind {
  if (name.startsWith("ctx_")) return "context";
  if (["read", "list", "search"].includes(name) || name.startsWith("lsp_")) return "filesRead";
  if (["write", "edit", "apply_patch"].includes(name)) return "filesWrite";
  if (name === "bash" || name.startsWith("process_") || name.startsWith("terminal_")) return "terminal";
  if (name === "web_search" || name.startsWith("browser_") || name.startsWith("context7_")) return "research";
  if (name === "read_skill" || name === "find_skills" || name.includes("skill")) return "skills";
  if (name.startsWith("hub_")) return "agents";
  if (name.startsWith("beads_") || name === "update_tasks" || name.startsWith("validation_") || name === "workflow_check") return "tasks";
  if (name.startsWith("design_")) return "design";
  if (["vision", "generate_image", "read_attachment"].includes(name)) return "media";
  if (name.startsWith("jarvis_")) return "jarvis";
  if (name.startsWith("mcp_")) return "integrations";
  return "other";
}

function naturalList(items: string[]): string {
  if (items.length === 1) return items[0];
  if (items.length === 2) return `${items[0]} e ${items[1]}`;
  return `${items.slice(0, -1).join(", ")} e ${items[items.length - 1]}`;
}

export function summarizeToolActivity(tools: ToolCallItem[]): string {
  const kinds = [...new Set(tools.map(tool => actionKind(tool.name)))];
  const visible: string[] = kinds.slice(0, 3).map(kind => actionLabels[kind]);
  if (kinds.length > 3) visible.push(`mais ${kinds.length - 3} ${kinds.length === 4 ? "tipo de ação" : "tipos de ação"}`);
  const summary = naturalList(visible);
  return summary.charAt(0).toUpperCase() + summary.slice(1);
}

export function groupToolActivity(tools: ToolCallItem[]): ToolActivityGroup[] {
  if (tools.length < GROUP_THRESHOLD) return [];
  const groups: ToolActivityGroup[] = [];
  for (let index = 0; index < tools.length; index += GROUP_SIZE) {
    const batch = tools.slice(index, index + GROUP_SIZE);
    groups.push({
      id: `${batch[0].id}:${batch[batch.length - 1]?.id}`,
      tools: batch,
      summary: summarizeToolActivity(batch),
      failures: batch.filter(tool => tool.status === "error").length,
      active: batch.some(tool => tool.status === "running" || tool.status === "pending"),
    });
  }
  return groups;
}
