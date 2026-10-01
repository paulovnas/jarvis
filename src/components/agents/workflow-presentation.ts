import { Film, Hammer, Palette, Route, Sparkles, Workflow } from "lucide-react";

export const BUILTIN_FLOWS = [
  { value: "standard", title: "Padrão", description: "Da sua instrução à implementação.", icon: Hammer, color: "#61afef" },
  { value: "designer", title: "Designer", description: "Implementação especializada em design e frontend.", icon: Palette, color: "#e06c9f" },
  { value: "video", title: "Vídeo", description: "Apresentações de projetos com animação, narração, música e renderização MP4.", icon: Film, color: "#56b6c2" },
  { value: "image_generator", title: "Imagens", description: "Imagens e variações com preparação de tamanho e formato pelo ComfyUI.", icon: Sparkles, color: "#98c379" },
  { value: "planned", title: "Planejado", description: "Planeje antes de construir e validar.", icon: Route, color: "#c678dd" },
  { value: "complete", title: "Completo", description: "Uma equipe da investigação à revisão.", icon: Workflow, color: "#e5c07b" },
] as const;
