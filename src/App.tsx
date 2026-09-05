import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArrowRight,
  FolderGit2,
  KeyRound,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { toast } from "sonner";
import { TitleBar, type TitleBarContext } from "@/components/layout/TitleBar";
import { JarvisLogo } from "@/components/JarvisLogo";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardHeader,
} from "@/components/ui/card";
import { Toaster } from "@/components/ui/sonner";
import { HomeSkeleton } from "@/components/layout/LoadingSkeletons";
import { DesktopLayoutProvider } from "@/components/layout/DesktopLayoutProvider";

const LazyHome = lazy(() => import("@/components/layout/Home"));

type AppConfig = {
  onboardingCompleted: boolean;
};

type BootstrapState =
  | { status: "loading" }
  | { status: "error" }
  | { status: "onboarding"; saving: boolean }
  | { status: "home" };

const SETUP_STEPS = [
  {
    icon: KeyRound,
    title: "Provedores de Inteligência",
    description: "Conecte suas contas OpenAI Codex ou Antigravity.",
    tag: "Etapa 1",
    accentClass: "text-[#e5c07b] bg-[#e5c07b]/10 border-[#e5c07b]/20",
  },
  {
    icon: FolderGit2,
    title: "Workspace & Repositório",
    description: "Escolha uma pasta e organize suas conversas.",
    tag: "Etapa 2",
    accentClass: "text-[#56b6c2] bg-[#56b6c2]/10 border-[#56b6c2]/20",
  },
  {
    icon: ShieldCheck,
    title: "Permissões & Ferramentas Locais",
    description: "Decida quando autorizar edições e comandos.",
    tag: "Etapa 3",
    accentClass: "text-[#98c379] bg-[#98c379]/10 border-[#98c379]/20",
  },
];

export function App() {
  useEffect(() => {
    // Suppress the WebView menu without stopping our scoped context-menu triggers.
    const preventNativeMenu = (event: MouseEvent) => event.preventDefault();
    document.addEventListener("contextmenu", preventNativeMenu);
    return () => document.removeEventListener("contextmenu", preventNativeMenu);
  }, []);
  const [bootstrap, setBootstrap] = useState<BootstrapState>({
    status: "loading",
  });
  const bootstrapRequest = useRef(0);
  const initialLoadStarted = useRef(false);
  const completionInFlight = useRef(false);

  const loadAppConfig = useCallback(() => {
    const requestId = ++bootstrapRequest.current;
    setBootstrap({ status: "loading" });

    void invoke<AppConfig>("get_app_config")
      .then((config) => {
        if (requestId !== bootstrapRequest.current) return;

        setBootstrap(
          config.onboardingCompleted === true
            ? { status: "home" }
            : { status: "onboarding", saving: false },
        );
      })
      .catch(() => {
        if (requestId === bootstrapRequest.current) {
          setBootstrap({ status: "error" });
        }
      });
  }, []);

  useEffect(() => {
    if (initialLoadStarted.current) return;
    initialLoadStarted.current = true;
    loadAppConfig();
  }, [loadAppConfig]);

  const handleCompleteOnboarding = async () => {
    if (
      bootstrap.status !== "onboarding" ||
      bootstrap.saving ||
      completionInFlight.current
    ) {
      return;
    }

    completionInFlight.current = true;
    setBootstrap({ status: "onboarding", saving: true });

    try {
      const config = await invoke<AppConfig>("complete_onboarding");
      if (config.onboardingCompleted === true) {
        setBootstrap({ status: "home" });
        return;
      }
    } catch {
      // Keep the onboarding screen recoverable without exposing backend details.
    } finally {
      completionInFlight.current = false;
    }

    setBootstrap({ status: "onboarding", saving: false });
    toast.error("Não foi possível concluir o onboarding", {
      description: "Tente novamente.",
    });
  };

  const titleBarContext: TitleBarContext =
    bootstrap.status === "home"
      ? "Início"
      : bootstrap.status === "onboarding"
        ? "Onboarding"
        : "Iniciando";

  const content = (() => {
    switch (bootstrap.status) {
      case "loading":
        return (
          <main className="flex min-h-0 flex-1"><HomeSkeleton /></main>
        );

      case "error":
        return (
          <main className="flex flex-1 items-center justify-center p-4 sm:p-6">
            <section
              role="alert"
              className="w-full max-w-md space-y-4 rounded-lg border border-[#e06c75]/40 bg-card p-6 text-center shadow-2xl shadow-black/30"
            >
              <div className="space-y-1">
                <h1 className="text-lg font-semibold text-foreground">
                  Não foi possível carregar o Jarvis
                </h1>
                <p className="text-sm text-muted-foreground">
                  Verifique a configuração local e tente novamente.
                </p>
              </div>
              <Button
                type="button"
                onClick={loadAppConfig}
                className="cursor-pointer"
              >
                Tentar novamente
              </Button>
            </section>
          </main>
        );

      case "home":
        return (
          <main className="flex min-h-0 flex-1">
            <Suspense fallback={<HomeSkeleton />}>
              <DesktopLayoutProvider><LazyHome /></DesktopLayoutProvider>
            </Suspense>
          </main>
        );

      case "onboarding":
        return (
          <main className="flex flex-1 flex-col items-center justify-center overflow-auto p-6">
            <div className="w-full max-w-lg space-y-5">
              <div className="flex items-center gap-3">
                <JarvisLogo className="size-9 text-foreground" />
                <span className="micro-label text-muted-foreground">Seu espaço de desenvolvimento</span>
              </div>
              <Card className="instrument-panel gap-0 overflow-hidden py-0">
                <CardHeader className="gap-3 border-b border-border p-6">
                  <Badge variant="outline" className="w-fit gap-1.5 border-primary/20 bg-primary/5 text-primary"><Sparkles className="size-3" />Primeiros passos</Badge>
                  <h1 className="text-2xl font-medium tracking-tight text-foreground">Bem-vindo ao Jarvis</h1>
                  <p className="text-sm leading-relaxed text-muted-foreground">Seus modelos, projetos e ferramentas. Em um só lugar.</p>
                </CardHeader>
                <CardContent className="flex flex-col gap-0 px-6 py-2">
                  {SETUP_STEPS.map((step) => {
                    const Icon = step.icon;
                    return <div key={step.title} className="flex items-start gap-3 border-b border-border py-4 last:border-b-0">
                      <div className={`flex size-8 shrink-0 items-center justify-center rounded-md border ${step.accentClass}`}><Icon className="size-4" /></div>
                      <div className="min-w-0 flex-1"><h2 className="text-xs font-medium">{step.title}</h2><p className="mt-1 text-xs leading-relaxed text-muted-foreground">{step.description}</p></div>
                      <span className="font-mono text-[10px] text-muted-foreground">{step.tag.replace("Etapa ", "0")}</span>
                    </div>;
                  })}
                </CardContent>
              </Card>
              <Button type="button" size="lg" onClick={handleCompleteOnboarding} disabled={bootstrap.saving} aria-busy={bootstrap.saving} className="w-full cursor-pointer font-medium">
                {bootstrap.saving ? "Finalizando..." : "Finalizar"}<ArrowRight className="size-4" />
              </Button>
            </div>
          </main>
        );
    }
  })();

  return (
    <div className="flex h-screen min-h-0 flex-col bg-background text-foreground font-sans">
      <TitleBar context={titleBarContext} />
      {content}
      {/* Global Toast Notification Provider */}
      <Toaster position="bottom-right" richColors />
    </div>
  );
}


export default App;
