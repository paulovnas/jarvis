import { Brain, Search, PenLine, Network, Palette, Hammer, ShieldCheck, Bot, GitPullRequest } from "lucide-react";

export const AGENT_ICONS = { planner: Brain, investigator: Search, writer: PenLine, orchestrator: Network, designer: Palette, builder: Hammer, reviewer: ShieldCheck, github: GitPullRequest, custom: Bot };
export const AGENT_DESCRIPTIONS = {
  custom: "Executa as instruções e permissões definidas pelo usuário para esta etapa.",
  planner: "Transforma seu pedido em etapas claras. Consulta o Beads, define critérios de aceite e organiza as dependências antes de delegar a execução.",
  investigator: "Investiga o código, as instruções e o histórico do projeto. Localiza evidências, identifica riscos e entrega os achados que orientam o planejamento.",
  writer: "Transforma o plano em uma especificação executável. Registra épicos, tarefas, critérios de aceite e dependências no Beads para orientar a implementação.",
  orchestrator: "Distribui as tarefas entre os agentes e acompanha as entregas. Coordena dependências, trabalho paralelo, revisões e retomadas quando algo falha.",
  designer: "Define a direção visual e implementa a experiência de uso. Consulta sistemas, templates e guias do Open Design, preservando a identidade do projeto.",
  builder: "Implementa o comportamento solicitado no código. Usa as ferramentas do projeto, executa os testes e corrige problemas antes de entregar o resultado.",
  reviewer: "Revisa a implementação em um contexto independente. Confere os critérios de aceite, executa as validações e aponta correções antes de aprovar a entrega.",
  github: "Inspeciona os repositórios alterados, executa as validações pertinentes e prepara commits, pull requests e merges para sua aprovação.",
};
