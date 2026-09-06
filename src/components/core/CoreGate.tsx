import { lazy, Suspense, useState, type ReactNode } from "react";
import { ShieldAlert, Wrench } from "lucide-react";
import { JarvisLogo } from "@/components/JarvisLogo";
import { Alert, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useCore } from "@/hooks/use-core";
import { HomeSkeleton } from "@/components/layout/LoadingSkeletons";

const CoreDiagnostics = lazy(() => import("./CoreDiagnostics"));

export function CoreGate({ children }: { children: ReactNode }) {
  const core = useCore();
  const [diagnostics, setDiagnostics] = useState(false);
  if (!core.snapshot && !core.error) return <HomeSkeleton />;
  if (core.snapshot?.ready && !core.error && !diagnostics) return children;
  const affected = core.snapshot?.items.filter(item => !item.installed || !item.configured || item.healthError).map(item => item.name).join(" · ");
  return <div className="flex min-h-0 flex-1 flex-col">
    <Alert variant="destructive" className="flex shrink-0 items-center gap-3 rounded-none border-x-0 border-t-0 border-destructive/30 bg-destructive/10 px-5 py-2">
      <ShieldAlert className="size-4 shrink-0" /><AlertTitle className="flex-1 text-xs">O Core precisa de atenção. Uso do Jarvis pausado.</AlertTitle>
      <Button size="sm" variant="outline" onClick={() => setDiagnostics(true)}><Wrench className="size-3.5" />Solucionar</Button>
    </Alert>
    <div className="m-auto max-w-lg space-y-4 p-8 text-center"><JarvisLogo className="mx-auto size-10 opacity-60" /><h1 className="text-lg font-medium">Vamos recuperar seu ambiente</h1><p className="text-sm text-muted-foreground">{core.error ?? affected ?? "Verifique os componentes do Core."}</p></div>
    {diagnostics && <Suspense fallback={<Skeleton aria-label="Carregando diagnóstico" className="mx-auto mb-8 h-52 w-3/4" />}><CoreDiagnostics core={core} onClose={() => setDiagnostics(false)} /></Suspense>}
  </div>;
}
