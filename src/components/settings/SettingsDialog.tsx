import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  ExternalLink,
  Link2,
  Plus,
  Settings,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { InputGroup, InputGroupAddon, InputGroupInput, InputGroupText } from "@/components/ui/input-group";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { accountList, type ProviderAccount } from "@/core/provider-accounts";


const ALIAS_PREFIX = "openai-codex-";
const ALIAS_SUFFIX_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const ACCOUNT_TYPE_LABELS: Record<ProviderAccount["accountType"], string> = {
  personal: "Pessoal",
  enterprise: "Enterprise",
  unknown: "Não identificado",
};


type SettingsView = "list" | "add" | "waiting";
type ListState = "loading" | "ready" | "error";


export type ProviderError = {
  code: string;
  message: string;
};

type ConnectionStart = {
  flowId: string;
  authorizationUrl: string;
};

type ActiveConnection = ConnectionStart & {
  alias: string;
};

type SettingsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAccountsChange?: (accounts: ProviderAccount[]) => void;
};

function validateAliasSuffix(value: string): string | null {
  if (value.length === 0) return "Informe um sufixo para o alias.";
  if (value.length > 32) return "Use entre 1 e 32 caracteres.";
  if (!ALIAS_SUFFIX_PATTERN.test(value)) {
    return "Use letras minúsculas, números e hífens internos.";
  }
  return null;
}

function safeErrorMessage(error: unknown, fallback: string): string {
  if (typeof error !== "object" || error === null) return fallback;
  const value = error as { code?: unknown; message?: unknown };
  return typeof value.code === "string" && typeof value.message === "string" && value.message.length > 0
    ? value.message
    : fallback;
}


function formatConnectionDate(timestamp: number): string {
  if (!timestamp || timestamp <= 0) return "Data indisponível";
  const date = new Date(timestamp * 1000);
  return date.toLocaleDateString("pt-BR", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}


export function SettingsDialog({ open, onOpenChange, onAccountsChange }: SettingsDialogProps) {
  const [activeTab, setActiveTab] = useState<string>("providers");
  const [view, setView] = useState<SettingsView>("list");
  const [listState, setListState] = useState<ListState>("loading");
  const [accounts, setAccounts] = useState<ProviderAccount[]>([]);
  const [listError, setListError] = useState<string | null>(null);
  const [suffix, setSuffix] = useState("");
  const [suffixError, setSuffixError] = useState<string | null>(null);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [disconnectAlias, setDisconnectAlias] = useState<string | null>(null);
  const [disconnecting, setDisconnecting] = useState<string | null>(null);
  const [disconnectError, setDisconnectError] = useState<string | null>(null);

  const activeConnectionRef = useRef<ActiveConnection | null>(null);
  const listRequestRef = useRef(0);
  const startingRef = useRef(false);
  const cancellingRef = useRef(false);
  const closeRequestedRef = useRef(false);
  const closingRef = useRef(false);

  const updateAccounts = useCallback(
    (result: ProviderAccount[] | undefined) => {
      const nextAccounts = accountList(result);
      setAccounts(nextAccounts);
      onAccountsChange?.(nextAccounts);
      setListState("ready");
    },
    [onAccountsChange],
  );

  const loadAccounts = useCallback(async () => {
    const requestId = ++listRequestRef.current;
    try {
      const result = await invoke<ProviderAccount[]>("list_provider_accounts");
      if (requestId !== listRequestRef.current) return;
      updateAccounts(result);
    } catch (error) {
      if (requestId !== listRequestRef.current) return;
      setListError(safeErrorMessage(error, "Não foi possível acessar as contas conectadas."));
      setListState("error");
    }
  }, [updateAccounts]);

  useEffect(() => {
    if (!open) return;
    const requestId = ++listRequestRef.current;
    void invoke<ProviderAccount[]>("list_provider_accounts").then(
      (result) => {
        if (requestId !== listRequestRef.current) return;
        updateAccounts(result);
      },
      (error: unknown) => {
        if (requestId !== listRequestRef.current) return;
        setListError(safeErrorMessage(error, "Não foi possível acessar as contas conectadas."));
        setListState("error");
      },
    );
  }, [open, updateAccounts]);

  const openAddView = () => {
    setView("add");
    setConnectionError(null);
    setSuffixError(null);
  };

  const handleSuffixChange = (value: string) => {
    setSuffix(value);
    setSuffixError(value.length === 0 ? null : validateAliasSuffix(value));
  };

  const clearActiveConnection = () => {
    activeConnectionRef.current = null;
  };

  const handleConnect = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (startingRef.current || activeConnectionRef.current) return;

    const validationError = validateAliasSuffix(suffix);
    setSuffixError(validationError);
    if (validationError) return;

    const alias = `${ALIAS_PREFIX}${suffix}`;
    startingRef.current = true;
    setStarting(true);
    setConnectionError(null);

    let started: ActiveConnection | null = null;
    try {
      const result = await invoke<ConnectionStart>("begin_openai_codex_connection", { alias });
      if (
        !result ||
        typeof result.flowId !== "string" ||
        result.flowId.length === 0 ||
        typeof result.authorizationUrl !== "string" ||
        result.authorizationUrl.length === 0
      ) {
        setConnectionError("Não foi possível iniciar a conexão.");
        return;
      }

      started = { ...result, alias };
      activeConnectionRef.current = started;

      if (closeRequestedRef.current) {
        try {
          await invoke<void>("cancel_openai_codex_connection", { flowId: started.flowId });
        } catch {
          // Closing remains best effort after the cancel command was attempted.
        } finally {
          clearActiveConnection();
          closeRequestedRef.current = false;
        }
        return;
      }

      setView("waiting");
      try {
        await openUrl(started.authorizationUrl);
      } catch {
        try {
          await invoke<void>("cancel_openai_codex_connection", { flowId: started.flowId });
        } catch {
          // Keep the user-facing failure redacted when browser launch fails.
        }
        if (activeConnectionRef.current?.flowId === started.flowId) {
          clearActiveConnection();
          setView("add");
          setConnectionError("Não foi possível abrir o navegador.");
        }
        return;
      }

      await invoke<ProviderAccount>("wait_openai_codex_connection", {
        flowId: started.flowId,
      });
      if (activeConnectionRef.current?.flowId !== started.flowId) return;

      clearActiveConnection();
      setListState("loading");
      setListError(null);
      setView("list");
      await loadAccounts();
      toast.success("Conta conectada");
    } catch (error) {
      if (!started) {
        setConnectionError(safeErrorMessage(error, "Não foi possível iniciar a conexão."));
      } else if (activeConnectionRef.current?.flowId === started.flowId) {
        clearActiveConnection();
        setView("add");
        const cancelled =
          typeof error === "object" &&
          error !== null &&
          "code" in error &&
          error.code === "cancelled";
        setConnectionError(
          cancelled ? null : safeErrorMessage(error, "Não foi possível conectar a conta."),
        );
      }
    } finally {
      startingRef.current = false;
      setStarting(false);
    }
  };

  const cancelActiveConnection = useCallback(
    async (closeAfter: boolean) => {
      const connection = activeConnectionRef.current;
      if (!connection) {
        if (closeAfter) onOpenChange(false);
        return;
      }
      if (cancellingRef.current) return;

      cancellingRef.current = true;
      setCancelling(true);
      try {
        await invoke<void>("cancel_openai_codex_connection", {
          flowId: connection.flowId,
        });
      } catch (error) {
        if (!closeAfter) {
          setConnectionError(
            safeErrorMessage(error, "Não foi possível cancelar a conexão."),
          );
        }
      } finally {
        if (closeAfter && activeConnectionRef.current?.flowId === connection.flowId) {
          clearActiveConnection();
        }
        cancellingRef.current = false;
        setCancelling(false);
        if (closeAfter) onOpenChange(false);
      }
    },
    [onOpenChange],
  );

  const handleDialogOpenChange = (nextOpen: boolean) => {
    if (nextOpen) {
      onOpenChange(true);
      return;
    }
    if (closingRef.current) return;

    if (startingRef.current && !activeConnectionRef.current) {
      closeRequestedRef.current = true;
      onOpenChange(false);
      return;
    }

    if (activeConnectionRef.current) {
      closingRef.current = true;
      void cancelActiveConnection(true).finally(() => {
        closingRef.current = false;
      });
      return;
    }

    onOpenChange(false);
  };

  const handleReopenBrowser = async () => {
    const connection = activeConnectionRef.current;
    if (!connection || cancellingRef.current) return;
    setConnectionError(null);
    try {
      await openUrl(connection.authorizationUrl);
    } catch {
      setConnectionError("Não foi possível abrir o navegador novamente.");
    }
  };

  const handleDisconnect = async () => {
    const alias = disconnectAlias;
    if (!alias || disconnecting) return;

    setDisconnecting(alias);
    setDisconnectError(null);
    try {
      await invoke<void>("disconnect_provider_account", { alias });
      setDisconnectAlias(null);
      setListState("loading");
      setListError(null);
      await loadAccounts();
      toast.success("Conta desconectada");
    } catch (error) {
      setDisconnectError(
        safeErrorMessage(error, "Não foi possível desconectar a conta."),
      );
    } finally {
      setDisconnecting(null);
    }
  };

  const renderList = () => {
    if (listState === "loading") {
      return (
        <div
          role="status"
          aria-live="polite"
          aria-label="Carregando contas conectadas"
          className="space-y-4"
        >
          <Skeleton className="h-32 w-full rounded-xl bg-[#21252b]" />
          <Skeleton className="h-32 w-full rounded-xl bg-[#21252b]" />
        </div>
      );
    }

    if (listState === "error") {
      return (
        <Card className="border-[#e06c75]/40 bg-[#e06c75]/5" role="alert">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-sm text-[#e06c75]">
              <AlertTriangle className="size-4" />
              Não foi possível carregar as contas
            </CardTitle>
            <CardDescription className="text-[#abb2bf]">{listError}</CardDescription>
          </CardHeader>
          <CardFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                setListState("loading");
                setListError(null);
                void loadAccounts();
              }}
              className="cursor-pointer border-[#e06c75]/40 text-[#e06c75] hover:bg-[#e06c75]/10"
            >
              Tentar novamente
            </Button>
          </CardFooter>
        </Card>
      );
    }

    return (
      <div className="space-y-5">
        <div className="flex items-center justify-between gap-3">
          <div>
            <h2 className="font-heading text-base font-semibold text-[#e6e6e6]">Provedores Conectados</h2>
            <p className="mt-0.5 text-xs text-[#7f848e]">
              Contas de assinatura para execução autônoma de modelos pelo Jarvis.
            </p>
          </div>
          {accounts.length > 0 && (
            <Button
              type="button"
              onClick={openAddView}
              className="cursor-pointer gap-1.5 bg-[#61afef] text-xs font-medium text-[#1e2227] hover:bg-[#61afef]/90"
            >
              <Plus className="size-3.5" />
              Adicionar conta
            </Button>
          )}
        </div>

        {accounts.length === 0 ? (
          <Empty className="min-h-72 rounded-xl border border-dashed border-[#3e4451] bg-[#21252b]/50">
            <EmptyHeader>
              <EmptyMedia variant="icon" className="bg-[#56b6c2]/10 text-[#56b6c2]">
                <Link2 className="size-5" />
              </EmptyMedia>
              <EmptyTitle className="text-base text-[#e6e6e6]">Nenhuma conta conectada</EmptyTitle>
              <EmptyDescription className="max-w-sm text-xs text-[#7f848e]">
                Conecte sua conta ChatGPT Plus ou Pro para habilitar os modelos de código no Jarvis sem custo de API por token.
              </EmptyDescription>
            </EmptyHeader>
            <EmptyContent>
              <Button type="button" onClick={openAddView} className="cursor-pointer gap-2 bg-[#61afef] text-xs text-[#1e2227] hover:bg-[#61afef]/90">
                <Plus className="size-4" />
                Adicionar conta
              </Button>
            </EmptyContent>
          </Empty>
        ) : (
          <div className="space-y-4">
            {accounts.map((account) => (
              <Card
                key={account.alias}
                data-testid={`provider-account-${account.alias}`}
                className="rounded-xl border-[#3e4451] bg-[#21252b] transition-all hover:border-[#61afef]/30"
              >
                <CardHeader className="gap-2 pb-3.5">
                  <div className="flex items-start justify-between gap-3">
                    <div className="flex items-center gap-3 min-w-0">
                      <div className="flex size-10 shrink-0 items-center justify-center rounded-xl border border-[#3e4451] bg-[#1e2227] text-[#56b6c2] shadow-xs">
                        <Bot className="size-5" />
                      </div>
                      <div className="min-w-0">
                        <CardTitle className="truncate font-mono text-sm font-semibold text-[#e6e6e6]">
                          {account.alias}
                        </CardTitle>
                        <CardDescription className="mt-0.5 text-xs text-[#7f848e]">
                          OpenAI Codex
                        </CardDescription>
                      </div>
                    </div>
                    <Badge className="shrink-0 border-[#98c379]/30 bg-[#98c379]/10 text-[10px] font-medium text-[#98c379] gap-1">
                      <CheckCircle2 className="size-3" />
                      Conectada
                    </Badge>
                  </div>
                </CardHeader>

                <CardContent className="space-y-4 py-3 text-xs">
                  <dl className="space-y-2">
                    <div className="flex items-start justify-between gap-4 border-b border-[#3e4451]/40 py-1">
                      <dt className="shrink-0 text-[#7f848e]">E-mail:</dt>
                      <dd className="break-all text-right text-[#abb2bf]">
                        {account.email ?? "Não informado pela OpenAI"}
                      </dd>
                    </div>
                    <div className="flex items-center justify-between gap-4 border-b border-[#3e4451]/40 py-1">
                      <dt className="text-[#7f848e]">Tipo de conta:</dt>
                      <dd className="text-[#abb2bf]">{ACCOUNT_TYPE_LABELS[account.accountType]}</dd>
                    </div>
                    <div className="flex items-center justify-between gap-4 py-1">
                      <dt className="text-[#7f848e]">Conectada em:</dt>
                      <dd className="text-right text-[#abb2bf]">
                        {formatConnectionDate(account.createdAt)}
                      </dd>
                    </div>
                  </dl>

                  <div className="border-t border-[#3e4451]/70 pt-3">
                    <div className="mb-2 flex items-center justify-between gap-3">
                      <span className="font-medium text-[#e6e6e6]">Modelos disponíveis</span>
                      {account.modelsAvailable && (
                        <Badge
                          variant="outline"
                          className="border-[#56b6c2]/30 bg-[#56b6c2]/10 text-[10px] text-[#56b6c2]"
                        >
                          {account.models.length}
                        </Badge>
                      )}
                    </div>
                    {!account.modelsAvailable ? (
                      <p className="text-[#e5c07b]">Não foi possível consultar os modelos agora.</p>
                    ) : account.models.length === 0 ? (
                      <p className="text-[#7f848e]">A assinatura não retornou modelos.</p>
                    ) : (
                      <div className="flex flex-wrap gap-1.5">
                        {account.models.map((model) => (
                          <Badge
                            key={model.id}
                            variant="outline"
                            title={model.id}
                            className="border-[#3e4451] bg-[#2c313a] font-mono text-[10px] text-[#abb2bf]"
                          >
                            {model.name}
                          </Badge>
                        ))}
                      </div>
                    )}
                  </div>
                </CardContent>

                <CardFooter className="justify-end border-t border-[#3e4451]/70 pt-3">
                  <Button
                    type="button"
                    variant="destructive"
                    onClick={() => {
                      setDisconnectAlias(account.alias);
                      setDisconnectError(null);
                    }}
                    className="cursor-pointer gap-1.5 text-xs h-8"
                  >
                    <ExternalLink className="size-3 rotate-45" />
                    Desconectar
                  </Button>
                </CardFooter>
              </Card>
            ))}
          </div>
        )}
      </div>
    );
  };

  const renderAdd = () => {
    const computedAlias = `${ALIAS_PREFIX}${suffix}`;
    return (
      <Card className="rounded-xl border-[#3e4451] bg-[#21252b]">
        <CardHeader>
          <CardTitle className="text-base text-[#e6e6e6]">Adicionar conta</CardTitle>
          <CardDescription className="text-xs text-[#abb2bf]">
            Vincule uma conta ChatGPT Plus ou Pro usando o fluxo OAuth seguro no navegador.
          </CardDescription>
        </CardHeader>
        <form onSubmit={handleConnect}>
          <CardContent className="space-y-5">
            <div className="space-y-2">
              <Label htmlFor="provider-alias-suffix" className="text-xs text-[#e6e6e6]">
                Sufixo do alias
              </Label>
              <InputGroup className="border-[#3e4451] bg-[#1e2227]">
                <InputGroupAddon className="border-r border-[#3e4451] bg-[#2c313a]/50 pl-2.5">
                  <InputGroupText className="font-mono text-xs text-[#56b6c2]">
                    {ALIAS_PREFIX}
                  </InputGroupText>
                </InputGroupAddon>
                <InputGroupInput
                  id="provider-alias-suffix"
                  value={suffix}
                  onChange={(event) => handleSuffixChange(event.currentTarget.value)}
                  placeholder="pessoal"
                  autoComplete="off"
                  disabled={starting}
                  aria-invalid={suffixError !== null}
                  aria-describedby="provider-alias-help provider-alias-error"
                  className="font-mono text-xs text-[#e6e6e6] placeholder:text-[#7f848e]"
                />
              </InputGroup>
              <p id="provider-alias-help" className="text-[11px] text-[#7f848e]">
                Use 1–32 caracteres: letras minúsculas, números e hífens internos.
              </p>
              {suffixError && (
                <p id="provider-alias-error" role="alert" className="text-xs text-[#e06c75]">
                  {suffixError}
                </p>
              )}
              <p className="text-xs text-[#abb2bf]">
                Alias completo: <code className="font-mono text-[#61afef]">{computedAlias}</code>
              </p>
            </div>

            <div className="rounded-lg border border-[#56b6c2]/25 bg-[#56b6c2]/5 p-3.5 text-xs leading-relaxed text-[#abb2bf]">
              <div className="flex items-center gap-1.5 font-medium text-[#56b6c2] mb-1">
                <ShieldCheck className="size-4" />
                <span>Autenticação Direta e Segura</span>
              </div>
              A autenticação usa sua assinatura ChatGPT Plus ou Pro no navegador oficial da OpenAI. As credenciais ficam guardadas exclusivamente no Keychain do macOS; a interface e o banco mantêm apenas os metadados da conta.
            </div>

            {connectionError && (
              <div role="alert" className="flex items-start gap-2 rounded-lg border border-[#e06c75]/35 bg-[#e06c75]/5 p-3 text-xs text-[#e06c75]">
                <AlertTriangle className="mt-0.5 size-4 shrink-0" />
                <span>{connectionError}</span>
              </div>
            )}
          </CardContent>
          <CardFooter className="justify-between gap-2 border-t border-[#3e4451]/70 pt-4">
            <Button
              type="button"
              variant="ghost"
              onClick={() => {
                setView("list");
                setConnectionError(null);
                setSuffixError(null);
              }}
              disabled={starting}
              className="cursor-pointer text-xs text-[#abb2bf] hover:bg-[#2c313a]"
            >
              Voltar
            </Button>
            <Button type="submit" disabled={starting} className="cursor-pointer gap-2 text-xs bg-[#61afef] text-[#1e2227] hover:bg-[#61afef]/90">
              <ShieldCheck className="size-3.5" />
              {starting ? "Iniciando conexão…" : connectionError ? "Tentar novamente" : "Conectar com ChatGPT"}
            </Button>
          </CardFooter>
        </form>
      </Card>
    );
  };

  const renderWaiting = () => (
    <Card className="rounded-xl border-[#61afef]/35 bg-[#21252b]">
      <CardHeader>
        <CardTitle className="text-base text-[#e6e6e6]">Conectar com ChatGPT</CardTitle>
        <CardDescription className="text-xs text-[#abb2bf]">
          A janela de autenticação foi aberta no navegador padrão.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex items-center gap-3 rounded-lg border border-[#61afef]/25 bg-[#61afef]/5 p-4" aria-live="polite">
          <Spinner aria-label="Aguardando autenticação no navegador" className="size-5 text-[#61afef]" />
          <div>
            <p className="text-sm font-medium text-[#e6e6e6]">Aguardando autenticação no navegador</p>
            <p className="text-xs text-[#7f848e] mt-0.5">Conclua o login na aba aberta do ChatGPT para autorizar o acesso.</p>
          </div>
        </div>
        {connectionError && (
          <p role="alert" className="text-xs text-[#e06c75]">
            {connectionError}
          </p>
        )}
      </CardContent>
      <CardFooter className="flex-col-reverse items-stretch gap-2 border-t border-[#3e4451]/70 pt-4 sm:flex-row sm:justify-end">
        <Button
          type="button"
          variant="outline"
          onClick={() => void cancelActiveConnection(false)}
          disabled={cancelling}
          className="cursor-pointer border-[#e06c75]/40 text-xs text-[#e06c75] hover:bg-[#e06c75]/10"
        >
          {cancelling ? "Cancelando conexão…" : "Cancelar conexão"}
        </Button>
        <Button
          type="button"
          variant="secondary"
          onClick={() => void handleReopenBrowser()}
          disabled={cancelling}
          className="cursor-pointer gap-2 text-xs"
        >
          <ExternalLink className="size-3.5" />
          Abrir navegador novamente
        </Button>
      </CardFooter>
    </Card>
  );

  return (
    <>
      <Sheet open={open} onOpenChange={handleDialogOpenChange}>
        <SheetContent
          side="left"
          showCloseButton
          className="dark flex flex-col h-full w-full sm:max-w-none md:w-[50vw] min-w-[500px] max-w-[850px] border-r border-[#3e4451] bg-[#1e2227] p-0 text-[#abb2bf] shadow-2xl overflow-hidden"
        >
          <SheetHeader className="border-b border-[#3e4451] bg-[#21252b] px-6 py-4.5">
            <SheetTitle className="text-xl font-heading font-semibold text-[#e6e6e6]">Configurações</SheetTitle>
            <SheetDescription className="sr-only">Painel de configurações do Jarvis</SheetDescription>
          </SheetHeader>

          <Tabs value={activeTab} onValueChange={setActiveTab} className="flex flex-col flex-1 min-h-0">
            <div className="border-b border-[#3e4451] bg-[#21252b]/60 px-6">
              <TabsList variant="line" className="h-11 gap-6 border-b-0 p-0">
                <TabsTrigger
                  value="providers"
                  className="cursor-pointer gap-2 border-b-2 border-transparent px-2 py-2.5 text-xs font-medium text-[#abb2bf] data-[state=active]:border-[#61afef] data-[state=active]:text-[#61afef] transition-colors"
                >
                  <Sparkles className="size-3.5 text-[#61afef]" />
                  <span>Provedores</span>
                  {accounts.length > 0 && (
                    <Badge className="border-[#61afef]/30 bg-[#61afef]/10 text-[10px] text-[#61afef] px-1.5 py-0">
                      {accounts.length}
                    </Badge>
                  )}
                </TabsTrigger>
                <TabsTrigger
                  value="general"
                  disabled
                  className="cursor-not-allowed gap-2 px-2 py-2.5 text-xs font-medium text-[#7f848e] opacity-60"
                >
                  <Settings className="size-3.5" />
                  <span>Geral</span>
                  <Badge variant="outline" className="border-[#7f848e]/30 text-[9px] text-[#7f848e] px-1 py-0">
                    Em breve
                  </Badge>
                </TabsTrigger>
              </TabsList>
            </div>

            <TabsContent value="providers" className="flex-1 min-h-0 m-0 p-0">
              <ScrollArea className="h-[calc(100vh-8.5rem)] px-6 py-6">
                {view === "list" && renderList()}
                {view === "add" && renderAdd()}
                {view === "waiting" && renderWaiting()}
              </ScrollArea>
            </TabsContent>
          </Tabs>
        </SheetContent>
      </Sheet>

      <AlertDialog
        open={disconnectAlias !== null}
        onOpenChange={(nextOpen) => {
          if (!nextOpen && !disconnecting) {
            setDisconnectAlias(null);
            setDisconnectError(null);
          }
        }}
      >
        <AlertDialogContent size="sm" className="dark border-[#3e4451] bg-[#21252b] text-[#abb2bf]">
          <AlertDialogHeader>
            <AlertDialogTitle className="text-[#e6e6e6]">Desconectar conta?</AlertDialogTitle>
            <AlertDialogDescription className="text-xs text-[#abb2bf]">
              A conta <code className="font-mono text-[#61afef]">{disconnectAlias}</code> será removida deste workspace.
            </AlertDialogDescription>
          </AlertDialogHeader>
          {disconnectError && (
            <p role="alert" className="text-xs text-[#e06c75]">
              {disconnectError}
            </p>
          )}
          <AlertDialogFooter>
            <AlertDialogCancel
              disabled={disconnecting !== null}
              className="cursor-pointer text-xs"
            >
              Cancelar
            </AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={disconnecting !== null}
              onClick={(event) => {
                event.preventDefault();
                void handleDisconnect();
              }}
              className="cursor-pointer text-xs"
            >
              {disconnecting ? "Desconectando…" : "Desconectar"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

export default SettingsDialog;
