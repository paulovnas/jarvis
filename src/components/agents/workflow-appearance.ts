import { BookOpen, Bot, Brain, CodeXml, Lightbulb, Palette, PenLine, Rocket, Route, Search, ShieldCheck, Sparkles, Target, Terminal, Workflow, Wrench } from "lucide-react";
import { agentAppearance, type WorkflowAppearance } from "@/core/workflow-appearance";

export const WORKFLOW_ICONS = {
  bot: { label: "Robô", Icon: Bot }, workflow: { label: "Fluxo", Icon: Workflow },
  route: { label: "Rota", Icon: Route }, brain: { label: "Cérebro", Icon: Brain },
  search: { label: "Pesquisa", Icon: Search }, code: { label: "Código", Icon: CodeXml },
  palette: { label: "Design", Icon: Palette }, shield: { label: "Escudo", Icon: ShieldCheck },
  terminal: { label: "Terminal", Icon: Terminal }, wrench: { label: "Ferramenta", Icon: Wrench },
  book: { label: "Livro", Icon: BookOpen }, sparkles: { label: "Estrelas", Icon: Sparkles },
  target: { label: "Alvo", Icon: Target }, pen: { label: "Escrita", Icon: PenLine },
  lightbulb: { label: "Ideia", Icon: Lightbulb }, rocket: { label: "Foguete", Icon: Rocket },
} as const;
export const WORKFLOW_COLORS = {
  blue: { label: "Azul", value: "var(--color-onedark-blue)" },
  green: { label: "Verde", value: "var(--color-onedark-green)" },
  cyan: { label: "Ciano", value: "var(--color-onedark-cyan)" },
  yellow: { label: "Amarelo", value: "var(--color-onedark-yellow)" },
  red: { label: "Vermelho", value: "var(--color-onedark-red)" },
  purple: { label: "Roxo", value: "var(--color-onedark-purple)" },
  neutral: { label: "Neutro", value: "var(--muted-foreground)" },
} as const;

export function workflowAppearance(value: WorkflowAppearance | null | undefined, fallback = agentAppearance) {
  const appearance = value ?? fallback;
  return { Icon: WORKFLOW_ICONS[appearance.icon].Icon, color: WORKFLOW_COLORS[appearance.color].value };
}
