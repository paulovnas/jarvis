import { useState } from "react";
import {
  ArrowRight,
  FolderGit2,
  KeyRound,
  ShieldCheck,
  Sparkles,
  Terminal,
} from "lucide-react";
import { JarvisLogo } from "@/components/JarvisLogo";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
} from "@/components/ui/card";
import { TitleBar } from "@/components/layout/TitleBar";
import { Toaster } from "@/components/ui/sonner";

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
  const [hasStarted, setHasStarted] = useState(false);

  const handleStartOnboarding = () => {
    setHasStarted(true);
    toast.info("Em breve", {
      description: "O assistente de configuração guiada do Jarvis estará disponível na próxima atualização.",
    });
  };

  const handleLearnMore = () => {
    toast.message("Base de Conhecimento", {
      description: "A arquitetura do Jarvis é inspirada nas decisões de docs/metis.",
    });
  };

  return (
    <div className="flex min-h-screen flex-col bg-background text-foreground font-sans">
      <TitleBar />

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
              onClick={handleStartOnboarding}
              className="cursor-pointer bg-[#61afef] text-[#1e2227] hover:bg-[#61afef]/90 font-semibold shadow-md shadow-[#61afef]/20 transition-all active:scale-[0.98]"
            >
              <span>{hasStarted ? "Configurando..." : "Começar Configuração"}</span>
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

    {/* Global Toast Notification Provider */}
    <Toaster position="bottom-right" richColors />
  </div>
);
}

export default App;
