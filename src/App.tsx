import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArrowRight,
  FolderGit2,
  KeyRound,
  ShieldCheck,
  Sparkles,
  Terminal,
} from "lucide-react";
import { toast } from "sonner";
import { TitleBar, type TitleBarContext } from "@/components/layout/TitleBar";
import { JarvisLogo } from "@/components/JarvisLogo";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
} from "@/components/ui/card";
import { Toaster } from "@/components/ui/sonner";

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
    description: "Configuração de chaves de API (OpenAI, Anthropic, Ollama e modelos locais).",
    tag: "Etapa 1",
    accentClass: "text-[#e5c07b] bg-[#e5c07b]/10 border-[#e5c07b]/20",
  },
  {
    icon: FolderGit2,
    title: "Workspace & Repositório",
    description: "Definição da pasta do projeto e análise da base de código pelo agente.",
    tag: "Etapa 2",
    accentClass: "text-[#56b6c2] bg-[#56b6c2]/10 border-[#56b6c2]/20",
  },
  {
    icon: ShieldCheck,
    title: "Permissões & Ferramentas Locais",
    description: "Ajuste seguro de execução de comandos no terminal e edição de arquivos.",
    tag: "Etapa 3",
    accentClass: "text-[#98c379] bg-[#98c379]/10 border-[#98c379]/20",
  },
];

export function App() {
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

  const handleLearnMore = () => {
    toast.message("Base de Conhecimento", {
      description: "A arquitetura do Jarvis é inspirada nas decisões de docs/metis.",
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
          <main className="flex flex-1 items-center justify-center p-4 sm:p-6">
            <div
              role="status"
              aria-live="polite"
              className="rounded-lg border border-border/80 bg-card px-6 py-5 text-center shadow-2xl shadow-black/30"
            >
              <p className="text-sm text-muted-foreground">
                Carregando configuração…
              </p>
            </div>
          </main>
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
            <Suspense
              fallback={
                <div className="flex h-full items-center justify-center p-4 sm:p-6">
                  <div
                    role="status"
                    aria-live="polite"
                    aria-atomic="true"
                    className="rounded-lg border border-border/80 bg-card px-6 py-5 text-center shadow-2xl shadow-black/30"
                  >
                    <p className="text-sm text-muted-foreground">
                      Carregando interface principal…
                    </p>
                  </div>
                </div>
              }
            >
              <LazyHome />
            </Suspense>
          </main>
        );

      case "onboarding":
        return (
          <main className="flex flex-1 flex-col items-center justify-center p-4 sm:p-6">
            <div className="w-full max-w-xl space-y-6">
              {/* Top Header Badge */}
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <span className="flex size-2 rounded-full bg-[#98c379] animate-pulse" />
                  <span className="text-xs font-mono uppercase tracking-wider text-muted-foreground">
                    Jarvis Core · Tauri v2
                  </span>
                </div>
                <Badge variant="outline" className="border-border text-xs font-mono">
                  One Dark Theme
                </Badge>
              </div>

              {/* Central Onboarding Card */}
              <Card className="border-border/80 bg-card shadow-2xl shadow-black/40">
                <CardHeader className="space-y-4 pb-4">
                  <div className="flex items-center justify-between">
                    <div className="flex size-12 items-center justify-center rounded-xl border border-[#3e4451] bg-[#21252b] shadow-inner">
                      <JarvisLogo className="size-7" />
                    </div>
                    <Badge className="bg-[#61afef]/15 text-[#61afef] border-[#61afef]/30 font-medium">
                      <Sparkles className="size-3 mr-1" /> Onboarding
                    </Badge>
                  </div>

                  <div>
                    <h1 className="text-2xl font-bold tracking-tight text-foreground sm:text-3xl">
                      Bem-vindo ao Jarvis
                    </h1>
                    <CardDescription className="mt-1.5 text-sm text-muted-foreground leading-relaxed">
                      Seu ambiente autônomo de engenharia de software com Rust e Tauri.
                      Vamos preparar seu setup inicial para conectar modelos e seu repositório.
                    </CardDescription>
                  </div>
                </CardHeader>

                <CardContent className="space-y-3 pt-2">
                  <p className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Etapas que serão configuradas
                  </p>

                  <div className="space-y-2.5">
                    {SETUP_STEPS.map((step) => {
                      const Icon = step.icon;
                      return (
                        <div
                          key={step.title}
                          className="flex items-start gap-3 rounded-lg border border-border/60 bg-[#282c34]/50 p-3 transition-colors hover:border-[#61afef]/40"
                        >
                          <div
                            className={`flex size-8 shrink-0 items-center justify-center rounded-md border ${step.accentClass}`}
                          >
                            <Icon className="size-4" />
                          </div>
                          <div className="flex-1 min-w-0">
                            <div className="flex items-center justify-between gap-2">
                              <h4 className="text-sm font-medium text-foreground">
                                {step.title}
                              </h4>
                              <span className="text-[11px] font-mono text-muted-foreground">
                                {step.tag}
                              </span>
                            </div>
                            <p className="mt-0.5 text-xs text-muted-foreground leading-normal">
                              {step.description}
                            </p>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </CardContent>

                <CardFooter className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3 pt-4 border-t border-border/60">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={handleLearnMore}
                    className="cursor-pointer border-border hover:bg-muted text-xs"
                  >
                    <Terminal className="size-3.5 mr-1 text-[#56b6c2]" />
                    Docs de referência
                  </Button>

                  <Button
                    type="button"
                    size="lg"
                    onClick={handleCompleteOnboarding}
                    disabled={bootstrap.saving}
                    aria-busy={bootstrap.saving}
                    className="cursor-pointer bg-[#61afef] text-[#1e2227] hover:bg-[#61afef]/90 font-semibold shadow-md shadow-[#61afef]/20 transition-all active:scale-[0.98]"
                  >
                    <span>{bootstrap.saving ? "Finalizando..." : "Finalizar"}</span>
                    <ArrowRight className="size-4 ml-1 stroke-[2.5]" />
                  </Button>
                </CardFooter>
              </Card>

              {/* Footer info */}
              <footer className="text-center text-xs text-muted-foreground/80 space-y-1">
                <p>
                  Fonte <span className="text-foreground font-medium">Roboto</span> · Design System{" "}
                  <span className="text-[#61afef] font-medium">One Dark</span>
                </p>
                <p className="text-[11px] text-muted-foreground/60">
                  Base de conhecimento obrigatória: <code className="text-foreground/80">docs/metis</code>
                </p>
              </footer>
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
