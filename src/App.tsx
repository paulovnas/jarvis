import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { TitleBar } from "@/components/layout/TitleBar";
import { StatusBar } from "@/components/layout/StatusBar";
import { Button } from "@/components/ui/button";
import { Toaster } from "@/components/ui/sonner";
import { BootSkeleton, HomeSkeleton } from "@/components/layout/LoadingSkeletons";
import { DesktopLayoutProvider } from "@/components/layout/DesktopLayoutProvider";
import { CoreGate } from "@/components/core/CoreGate";
import { useNotificationFeedback } from "@/hooks/use-notification-feedback";

const LazyHome = lazy(() => import("@/components/layout/Home"));
const Onboarding = lazy(() => import("@/components/onboarding/Onboarding").then(module => ({ default: module.Onboarding })));

type AppConfig = {
  onboardingCompleted: boolean;
};

type BootstrapState =
  | { status: "loading" }
  | { status: "error" }
  | { status: "onboarding"; saving: boolean }
  | { status: "home" };



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

  const handleCompleteOnboarding = async (workspaceName: string) => {
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
      const config = await invoke<AppConfig>("complete_onboarding", { workspaceName });
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

  const content = (() => {
    switch (bootstrap.status) {
      case "loading":
        return (
          <main className="flex min-h-0 flex-1"><BootSkeleton /></main>
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
              <CoreGate><DesktopLayoutProvider><LazyHome /></DesktopLayoutProvider></CoreGate>
            </Suspense>
          </main>
        );

      case "onboarding":
        return <Suspense fallback={<main className="flex min-h-0 flex-1"><BootSkeleton /></main>}><Onboarding saving={bootstrap.saving} onComplete={handleCompleteOnboarding} /></Suspense>;
    }
  })();

  return (
    <div className="flex h-dvh min-h-0 flex-col overflow-hidden bg-background text-foreground font-sans">
      <TitleBar />
      {content}
      {bootstrap.status !== "home" && <StatusBar />}
      {/* Global Toast Notification Provider */}
      <Toaster position="top-center" richColors />
    </div>
  );
}


export default App;
