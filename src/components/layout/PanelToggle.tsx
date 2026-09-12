import { PanelLeftClose, PanelLeftOpen, PanelRightClose, PanelRightOpen } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Hint } from "@/components/ui/hint";

export function PanelToggle({ side, collapsed, onToggle }: { side: "left" | "right"; collapsed: boolean; onToggle: () => void }) {
  const label = `${collapsed ? "Abrir" : "Recolher"} ${side === "left" ? "barra lateral" : "inspector"}`;
  const Icon = side === "left" ? collapsed ? PanelLeftOpen : PanelLeftClose : collapsed ? PanelRightOpen : PanelRightClose;
  return <Hint content={label}><Button variant="ghost" size="icon-sm" className="shrink-0 cursor-pointer text-muted-foreground hover:text-foreground" aria-label={label} aria-expanded={!collapsed} aria-controls={side === "left" ? "workspace-panel-content" : "inspector-panel-content"} onClick={onToggle}><Icon className="size-4" /></Button></Hint>;
}
