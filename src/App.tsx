import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { BootstrapResourcesProvider } from "@/components/bootstrap/BootstrapResourcesProvider";
import { JarvisLogo } from "@/components/JarvisLogo";
import { TitleBar } from "@/components/layout/TitleBar";
import { StatusBar } from "@/components/layout/StatusBar";
import { Button } from "@/components/ui/button";
import { Toaster } from "@/components/ui/sonner";
import { HomeSkeleton } from "@/components/layout/LoadingSkeletons";
import { DesktopLayoutProvider } from "@/components/layout/DesktopLayoutProvider";
import { CoreGate } from "@/components/core/CoreGate";
import {
  type AppConfig,
  type BootstrapResources,
} from "@/core/bootstrap";
import { initialBootstrapProgress, type BootstrapProgressState } from "@/core/bootstrap-state";
import { useNotificationFeedback } from "@/hooks/use-notification-feedback";

const loadHome = () => import("@/components/layout/Home");
const LazyHome = lazy(loadHome);
const BootstrapScreen = lazy(() => import("@/components/bootstrap/BootstrapScreen").then(module => ({ default: module.BootstrapScreen })));
const Onboarding = lazy(() => import("@/components/onboarding/Onboarding").then(module => ({ default: module.Onboarding })));

type BootstrapState =
  | { status: "loading"; steps: BootstrapProgressState }
  | { status: "error"; steps: BootstrapProgressState }
  | { status: "onboarding"; saving: boolean; steps: BootstrapProgressState }
  | { status: "home"; resources: BootstrapResources };

function BootstrapFallback() {
  return <main role="status" aria-label="Iniciando o Jarvis" className="dark flex min-h-0 flex-1 items-center justify-center bg-background"><div className="flex flex-col items-center gap-3"><JarvisLogo className="size-14 opacity-80" /><span className="micro-label text-muted-foreground">Preparando ambiente</span></div></main>;
}

function bootstrapScreen(steps: BootstrapProgressState) {
  return <Suspense fallback={<BootstrapFallback />}><BootstrapScreen steps={steps} /></Suspense>;
}

export function App() {
  useNotificationFeedback();
  useEffect(() => {
    // Suppress the WebView menu without stopping our scoped context-menu triggers.
    const preventNativeMenu = (event: MouseEvent) => event.preventDefault();
    document.addEventListener("contextmenu", preventNativeMenu);
    return () => document.removeEventListener("contextmenu", preventNativeMenu);
  }, []);
  const [bootstrap, setBootstrap] = useState<BootstrapState>({
    status: "loading",
    steps: initialBootstrapProgress(),
  });
  const bootstrapRequest = useRef(0);
  const initialLoadStarted = useRef(false);
  const completionInFlight = useRef(false);

  const loadAppConfig = useCallback((knownConfig?: AppConfig) => {
    const requestId = ++bootstrapRequest.current;
    const steps = initialBootstrapProgress();
    setBootstrap({ status: "loading", steps });

    void import("@/core/bootstrap").then(({ loadAppBootstrap }) => loadAppBootstrap((event) => {
      if (requestId !== bootstrapRequest.current) return;
      setBootstrap((current) => current.status === "loading"
        ? { status: "loading", steps: { ...current.steps, [event.id]: event } }
        : current);
    }, knownConfig))
      .then(async ({ config, resources }) => {
        if (requestId !== bootstrapRequest.current) return;
        if (config.onboardingCompleted && resources) {
          await loadHome();
          if (requestId === bootstrapRequest.current) setBootstrap({ status: "home", resources });
          return;
        }
        setBootstrap((current) => ({
          status: "onboarding",
          saving: false,
          steps: current.status === "loading" ? current.steps : steps,
        }));
      })
      .catch(() => {
        if (requestId === bootstrapRequest.current) {
          setBootstrap((current) => ({
            status: "error",
            steps: current.status === "loading" ? current.steps : steps,
          }));
        }
      });
  }, []);

  useEffect(() => {
    if (initialLoadStarted.current) return;
    initialLoadStarted.current = true;
    loadAppConfig();
  }, [loadAppConfig]);

  const handleCompleteOnboarding = async (workspaceName: string) => {
    if (
      bootstrap.status !== "onboarding" ||
      bootstrap.saving ||
      completionInFlight.current
    ) {
      return;
    }

    completionInFlight.current = true;
    setBootstrap({ ...bootstrap, saving: true });

    try {
      const config = await invoke<AppConfig>("complete_onboarding", { workspaceName });
      if (config.onboardingCompleted === true) {
        loadAppConfig(config);
        return;
      }
    } catch {
      // Keep the onboarding screen recoverable without exposing backend details.
    } finally {
      completionInFlight.current = false;
    }

    setBootstrap({ ...bootstrap, saving: false });
    toast.error("Não foi possível concluir o onboarding", {
      description: "Tente novamente.",
    });
  };

  const content = (() => {
    switch (bootstrap.status) {
      case "loading":
        return bootstrapScreen(bootstrap.steps);

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
                onClick={() => loadAppConfig()}
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
              <BootstrapResourcesProvider initial={bootstrap.resources}>
                <CoreGate><DesktopLayoutProvider><LazyHome /></DesktopLayoutProvider></CoreGate>
              </BootstrapResourcesProvider>
            </Suspense>
          </main>
        );

      case "onboarding":
        return <Suspense fallback={bootstrapScreen(bootstrap.steps)}><Onboarding saving={bootstrap.saving} onComplete={handleCompleteOnboarding} /></Suspense>;
    }
  })();

  return (
    <div className="flex h-dvh min-h-0 flex-col overflow-hidden bg-background text-foreground font-sans">
      <TitleBar />
      {content}
      {bootstrap.status !== "home" && <StatusBar passive />}
      {/* Global Toast Notification Provider */}
      <Toaster position="top-center" richColors />
    </div>
  );
}


export default App;
