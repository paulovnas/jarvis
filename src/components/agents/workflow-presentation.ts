import { Film, Hammer, Palette, Route, Workflow } from "lucide-react";

export const BUILTIN_FLOWS = [
  { value: "standard", title: "Padrão", description: "Da sua instrução à implementação.", icon: Hammer, color: "#61afef" },
  { value: "designer", title: "Designer", description: "Implementação especializada em design e frontend.", icon: Palette, color: "#e06c9f" },
  { value: "video", title: "Vídeo", description: "Criação e renderização de vídeos com Hyperframes.", icon: Film, color: "#56b6c2" },
  { value: "planned", title: "Planejado", description: "Planeje antes de construir e validar.", icon: Route, color: "#c678dd" },
  { value: "complete", title: "Completo", description: "Uma equipe da investigação à revisão.", icon: Workflow, color: "#e5c07b" },
] as const;
