import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { pluginMonogram } from "@/core/plugins";
import { cn } from "@/lib/utils";

export function PluginIcon({ name, src, large = false }: { name: string; src?: string | null; large?: boolean }) {
  return <Avatar role="img" aria-label={`Ícone de ${name}`} className={cn("shrink-0 rounded-lg", large ? "size-12" : "size-10")}>
    {src && <AvatarImage src={src} alt="" className="rounded-lg object-contain" />}
    <AvatarFallback className="rounded-lg font-mono">{pluginMonogram(name)}</AvatarFallback>
  </Avatar>;
}
