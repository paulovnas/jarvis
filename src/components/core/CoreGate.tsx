import { lazy, Suspense, type ReactNode } from "react";
import { JarvisLogo } from "@/components/JarvisLogo";
import { Skeleton } from "@/components/ui/skeleton";
import { useCore } from "@/hooks/use-core";
import { HomeSkeleton } from "@/components/layout/LoadingSkeletons";

const CorePanel = lazy(() => import("@/components/settings/CoreSettings").then(module => ({ default: module.CorePanel })));

function CoreSkeleton() {
  return <div role="status" aria-label="Carregando Core" className="grid gap-3 sm:grid-cols-2">{[0, 1, 2, 3].map(id => <Skeleton key={id} className="h-56 w-full rounded-lg" />)}</div>;
}

export function CoreGate({ children }: { children: ReactNode }) {
  const core = useCore();
  if (!core.snapshot && !core.error) return <HomeSkeleton />;
  if (core.snapshot?.ready) return children;
  return <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto p-6"><div className="w-full max-w-xl space-y-6 py-6"><div className="flex items-center gap-3"><JarvisLogo className="size-8" /><div><h1 className="text-lg font-medium tracking-tight">Prepare seu Jarvis</h1><span className="micro-label text-muted-foreground">Configuração inicial</span></div></div><Suspense fallback={<CoreSkeleton />}><CorePanel core={core} setup /></Suspense></div></div>;
}
