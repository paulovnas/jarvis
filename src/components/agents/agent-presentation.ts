import { Brain, Search, PenLine, Network, Palette, Film, Hammer, ShieldCheck, Bot, GitPullRequest, Sparkles } from "lucide-react";

export const AGENT_ICONS = { planner: Brain, investigator: Search, writer: PenLine, orchestrator: Network, designer: Palette, video: Film, image_generator: Sparkles, builder: Hammer, reviewer: ShieldCheck, github: GitPullRequest, custom: Bot };
export const AGENT_DESCRIPTIONS = {
  custom: "Executa as instruções e permissões definidas pelo usuário para esta etapa.",
  planner: "Transforma seu pedido em etapas claras. Consulta o Beads, define critérios de aceite e organiza as dependências antes de delegar a execução.",
  investigator: "Investiga o código, as instruções e o histórico do projeto. Localiza evidências, identifica riscos e entrega os achados que orientam o planejamento.",
  writer: "Transforma o plano em uma especificação executável. Registra épicos, tarefas, critérios de aceite e dependências no Beads para orientar a implementação.",
  orchestrator: "Distribui as tarefas entre os agentes e acompanha as entregas. Coordena dependências, trabalho paralelo, revisões e retomadas quando algo falha.",
  designer: "Une direção de produto, UI/UX e implementação com Impeccable. Preserva a identidade do projeto, revisa a qualidade e permite experimentar variantes no modo Live.",
  video: "Ajuda a definir público, mensagem e roteiro. Dirige demonstrações com navegação, animação, narração e música e entrega o vídeo em MP4.",
  image_generator: "Gera imagens e variações com o provedor configurado. Usa o ComfyUI para ajustar tamanho, remover fundos e entregar assets em PNG, JPG ou WEBP.",
  builder: "Implementa o comportamento solicitado no código. Usa as ferramentas do projeto, executa os testes e corrige problemas antes de entregar o resultado.",
  reviewer: "Revisa a implementação em um contexto independente. Confere os critérios de aceite, executa as validações e aponta correções antes de aprovar a entrega.",
  github: "Inspeciona os repositórios alterados, executa as validações pertinentes e prepara commits, pull requests e merges para sua aprovação.",
};
