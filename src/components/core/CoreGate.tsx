import type { ReactNode } from "react";
import { JarvisLogo } from "@/components/JarvisLogo";
import { CorePanel } from "@/components/settings/CoreSettings";
import { useCore } from "@/hooks/use-core";
import { HomeSkeleton } from "@/components/layout/LoadingSkeletons";

export function CoreGate({ children }: { children: ReactNode }) {
  const core = useCore();
  if (!core.snapshot && !core.error) return <HomeSkeleton />;
  if (core.snapshot?.ready) return children;
  return <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto p-6"><div className="w-full max-w-xl space-y-6 py-6"><div className="flex items-center gap-3"><JarvisLogo className="size-8" /><div><h1 className="text-lg font-medium tracking-tight">Prepare seu Jarvis</h1><span className="micro-label text-muted-foreground">Configuração inicial</span></div></div><CorePanel core={core} setup /></div></div>;
}
