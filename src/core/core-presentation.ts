import { BookOpen, Braces, BrainCircuit, GitBranch, Palette, Zap } from "lucide-react";

export const CORE_DETAILS = {
  context7: { icon: BookOpen, label: "Documentação", description: "Consulta APIs e exemplos atualizados de bibliotecas. Dá ao Investigador referências precisas para cada versão.", color: "text-onedark-green", tint: "border-onedark-green/25 bg-onedark-green/10" },
  "context-mode": { icon: BrainCircuit, label: "Memória", description: "Indexa e recupera informações do projeto sob demanda, preservando espaço no contexto da conversa.", color: "text-primary", tint: "border-primary/25 bg-primary/10" },
  ponytail: { icon: Zap, label: "Precisão", description: "Orienta a escrita e a revisão de código com diretrizes de programação, reduzindo ruído e retrabalho.", color: "text-onedark-yellow", tint: "border-onedark-yellow/25 bg-onedark-yellow/10" },
  beads: { icon: GitBranch, label: "Planejamento", description: "Organiza épicos, tarefas e dependências. Mantém o progresso do projeto entre conversas e agentes.", color: "text-onedark-cyan", tint: "border-onedark-cyan/25 bg-onedark-cyan/10" },
  "open-design": { icon: Palette, label: "Design", description: "Sistemas visuais, templates e guias de acabamento para o Designer. Referências consultadas sob demanda.", color: "text-onedark-purple", tint: "border-onedark-purple/25 bg-onedark-purple/10" },
  lsp: { icon: Braces, label: "Navegação de código", description: "Instala servidores para localizar definições, referências, símbolos e diagnósticos em projetos TypeScript, JavaScript e Python.", color: "text-onedark-blue", tint: "border-onedark-blue/25 bg-onedark-blue/10" },
};
