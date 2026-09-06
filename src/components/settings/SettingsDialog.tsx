import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { WebSearchSettings } from "./WebSearchSettings";
import { McpSettings } from "./McpSettings";
import { SkillsSettings } from "./SkillsSettings";
import { CoreSettings } from "./CoreSettings";
import { ChatCleanupSettings } from "./ChatCleanupSettings";
import { AgentSettings } from "./AgentSettings";
import { skillsSnapshotSchema } from "@/core/skills";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  AlertTriangle,
  Users,
  BookOpen,
  ExternalLink,
  Link2,
  Plus,
  Plug,
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
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { mcpServersSchema } from "@/core/mcp";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { InputGroup, InputGroupAddon, InputGroupText } from "@/components/ui/input-group";
import { InputGroupInput } from "@/components/TextInput";
import { Label } from "@/components/ui/label";
import { CardsSkeleton } from "@/components/layout/LoadingSkeletons";
import { Spinner } from "@/components/ui/spinner";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { accountList, type ProviderAccount } from "@/core/provider-accounts";
import { ProviderAccountCard } from "./ProviderAccountCard";
import { CustomProviderForm } from "./CustomProviderForm";
import { useDesktopLayout } from "@/hooks/use-desktop-layout";


const ALIAS_SUFFIX_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;


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


export function SettingsDialog({ open, onOpenChange, onAccountsChange }: SettingsDialogProps) {
  const { layout, updateLayout } = useDesktopLayout();
  const activeTab = layout.settingsTab;
  const setActiveTab = (value: string) => { if (value === "general" || value === "providers" || value === "agents" || value === "skills" || value === "mcps") updateLayout({ settingsTab: value }); };
  const [mcpCount, setMcpCount] = useState<number | null>(null);
  const [skillCount, setSkillCount] = useState<number | null>(null);
  const skillCountVersion = useRef(0);
  const updateSkillCount = useCallback((count: number) => { skillCountVersion.current += 1; setSkillCount(count); }, []);
  const mcpCountVersion = useRef(0);
  const updateMcpCount = useCallback((count: number) => {
    mcpCountVersion.current += 1;
    setMcpCount(count);
  }, []);
  const [view, setView] = useState<SettingsView>("list");
  const [listState, setListState] = useState<ListState>("loading");
  const [accounts, setAccounts] = useState<ProviderAccount[]>([]);
  const [listError, setListError] = useState<string | null>(null);
  const [suffix, setSuffix] = useState("");
  const [provider, setProvider] = useState("openai-codex");
  const [editingCustom, setEditingCustom] = useState<ProviderAccount | null>(null);
  const [savingCustom, setSavingCustom] = useState(false);
  const aliasPrefix = `${provider}-`;
  const connectionLabel = provider === "antigravity" ? "Antigravity" : "ChatGPT";
  const [suffixError, setSuffixError] = useState<string | null>(null);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [disconnectAlias, setDisconnectAlias] = useState<string | null>(null);
  const [disconnecting, setDisconnecting] = useState<string | null>(null);
  const [disconnectError, setDisconnectError] = useState<string | null>(null);
  const [toggling, setToggling] = useState(false);
  const togglingRef = useRef(false);

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

  useEffect(() => {
    if (!open) return;
    let active = true;
    const version = mcpCountVersion.current;
    void invoke<unknown>("list_mcp_servers").then((value) => {
      if (active && version === mcpCountVersion.current) setMcpCount(mcpServersSchema.parse(value).length);
    }).catch(() => { if (active && version === mcpCountVersion.current) setMcpCount(null); });
    return () => { active = false; };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    let active = true;
    const version = skillCountVersion.current;
    void invoke("list_skills").then(value => {
      if (active && version === skillCountVersion.current) setSkillCount(skillsSnapshotSchema.parse(value).skills.length);
    }).catch(() => { if (active && version === skillCountVersion.current) setSkillCount(null); });
    return () => { active = false; };
  }, [open]);

  const openAddView = () => {
    if (startingRef.current || cancellingRef.current) return;
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

    const alias = `${aliasPrefix}${suffix}`;
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
      closeRequestedRef.current = false;
      setStarting(false);
    }
  };

  const cancelActiveConnection = useCallback(
    async (closeAfter: boolean, closeSettings = true) => {
      const close = () => { setView("list"); if (closeSettings) onOpenChange(false); };
      const connection = activeConnectionRef.current;
      if (!connection) {
        if (closeAfter) close();
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
        if (closeAfter) close();
      }
    },
    [onOpenChange],
  );

  const handleDialogOpenChange = (nextOpen: boolean, closeSettings = true) => {
    if (savingCustom && !nextOpen) return;
    if (nextOpen) {
      onOpenChange(true);
      return;
    }
    if (closingRef.current) return;

    if (startingRef.current && !activeConnectionRef.current) {
      closeRequestedRef.current = true;
      setView("list");
      if (closeSettings) onOpenChange(false);
      return;
    }

    if (activeConnectionRef.current) {
      closingRef.current = true;
      void cancelActiveConnection(true, closeSettings).finally(() => {
        closingRef.current = false;
      });
      return;
    }

    setView("list");
    if (closeSettings) onOpenChange(false);
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

  const handleEnabledChange = async (alias: string, enabled: boolean) => {
    if (togglingRef.current) return;
    togglingRef.current = true;
    setToggling(true);
    try {
      await invoke("set_provider_enabled", { alias, enabled });
      updateAccounts(accounts.map((account) => account.alias === alias ? { ...account, enabled } : account));
      await loadAccounts();
      toast.success(enabled ? "Conta ativada" : "Conta desativada");
    } catch {
      toast.error("Não foi possível alterar a ativação da conta.");
    } finally {
      togglingRef.current = false;
      setToggling(false);
    }
  };

  const handleUsageChange = async (alias: string, showUsage: boolean, showThirdPartyUsage: boolean) => {
    if (togglingRef.current) return;
    togglingRef.current = true; setToggling(true);
    try {
      await invoke("set_provider_usage_visibility", { alias, showUsage, showThirdPartyUsage });
      updateAccounts(accounts.map(account => account.alias === alias ? { ...account, showUsage, showThirdPartyUsage } : account));
    } catch { toast.error("Não foi possível salvar a visualização dos limites."); }
    finally { togglingRef.current = false; setToggling(false); }
  };

  const renderList = () => {
    if (listState === "loading") {
      return (
        <CardsSkeleton label="Carregando contas conectadas" columns />
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
            <CardDescription className="text-foreground">{listError}</CardDescription>
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
      <div className="flex flex-col gap-4">
        <div className="flex items-center justify-between gap-3">
          <div>
            <h2 className="micro-label text-muted-foreground">Provedores conectados</h2>
          </div>
          {accounts.length > 0 && (
            <Button
              type="button"
              onClick={openAddView}
              className="cursor-pointer gap-1.5 bg-[#61afef] text-xs font-medium text-primary-foreground hover:bg-[#61afef]/90"
            >
              <Plus className="size-3.5" />
              Adicionar conta
            </Button>
          )}
        </div>

        {accounts.length === 0 ? (
          <Empty className="min-h-72 rounded-lg border border-dashed border-border bg-card/50">
            <EmptyHeader>
              <EmptyMedia variant="icon" className="bg-[#56b6c2]/10 text-[#56b6c2]">
                <Link2 className="size-5" />
              </EmptyMedia>
              <EmptyTitle className="text-base text-foreground">Nenhuma conta conectada</EmptyTitle>
              <EmptyDescription className="max-w-sm text-xs text-muted-foreground">
                Adicione uma conta para acessar seus modelos.
              </EmptyDescription>
            </EmptyHeader>
            <EmptyContent>
              <Button type="button" onClick={openAddView} className="cursor-pointer gap-2 bg-[#61afef] text-xs text-primary-foreground hover:bg-[#61afef]/90">
                <Plus className="size-4" />
                Adicionar conta
              </Button>
            </EmptyContent>
          </Empty>
        ) : (
          <div className="grid items-start gap-3 sm:grid-cols-2">
            {accounts.map((account) => (
              <ProviderAccountCard
                key={account.alias}
                account={account}
                saving={toggling}
                onEdit={setEditingCustom}
                onUsageChange={(alias, showUsage, showThirdPartyUsage) => { void handleUsageChange(alias, showUsage, showThirdPartyUsage); }}
                onEnabledChange={(alias, enabled) => { void handleEnabledChange(alias, enabled); }}
                onDisconnect={(alias) => {
                  setDisconnectAlias(alias);
                  setDisconnectError(null);
                }}
              />
            ))}
          </div>
        )}
        <section className="space-y-3" aria-label="Ferramentas"><h3 className="micro-label text-muted-foreground">Ferramentas</h3><div className="grid gap-3 sm:grid-cols-2"><WebSearchSettings accounts={accounts} /><WebSearchSettings accounts={accounts} kind="vision" /></div></section>
      </div>
    );
  };

  const customSaved = (account: ProviderAccount) => {
    ++listRequestRef.current;
    updateAccounts(accounts.some(item => item.alias === account.alias) ? accounts.map(item => item.alias === account.alias ? account : item) : [...accounts, account]);
    setView("list"); setEditingCustom(null);
  };

  const renderAdd = () => {
    const computedAlias = `${aliasPrefix}${suffix}`;
    const providerSelect = <div className="space-y-2"><Label htmlFor="account-provider">Provedor</Label><Select value={provider} onValueChange={value => { if (value) { setProvider(value); setConnectionError(null); } }} disabled={starting || savingCustom}><SelectTrigger id="account-provider" className="w-full cursor-pointer"><SelectValue>{provider === "custom" ? "Custom" : provider === "antigravity" ? "Antigravity" : "OpenAI Codex"}</SelectValue></SelectTrigger><SelectContent><SelectItem value="openai-codex" className="cursor-pointer">OpenAI Codex</SelectItem><SelectItem value="antigravity" className="cursor-pointer">Antigravity</SelectItem><SelectItem value="custom" className="cursor-pointer">Custom</SelectItem></SelectContent></Select></div>;
    if (provider === "custom") return <><DialogHeader><DialogTitle>Adicionar conta</DialogTitle><DialogDescription>Configure seu endpoint e os limites informados pelo provedor.</DialogDescription></DialogHeader>{providerSelect}<CustomProviderForm onBusyChange={setSavingCustom} onCancel={() => setView("list")} onSaved={customSaved} /></>;
    return (
      <>
        <DialogHeader>
          <DialogTitle>Adicionar conta</DialogTitle>
          <DialogDescription>
            Conecte pelo navegador.
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={handleConnect}>
          <div className="space-y-5">
            {providerSelect}
            <div className="space-y-2">
              <Label htmlFor="provider-alias-suffix" className="text-xs text-foreground">
                Sufixo do alias
              </Label>
              <InputGroup className="border-border bg-sidebar">
                <InputGroupAddon className="border-r border-border bg-secondary/50 pl-2.5">
                  <InputGroupText className="font-mono text-xs text-[#56b6c2]">
                    {aliasPrefix}
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
                  className="font-mono text-xs text-foreground placeholder:text-muted-foreground"
                />
              </InputGroup>
              <p id="provider-alias-help" className="text-[11px] text-muted-foreground">
                Use 1–32 caracteres: letras minúsculas, números e hífens internos.
              </p>
              {suffixError && (
                <p id="provider-alias-error" role="alert" className="text-xs text-[#e06c75]">
                  {suffixError}
                </p>
              )}
              <p className="text-xs text-foreground">
                Alias completo: <code className="font-mono text-[#61afef]">{computedAlias}</code>
              </p>
            </div>

            {connectionError && (
              <div role="alert" className="flex items-start gap-2 rounded-lg border border-[#e06c75]/35 bg-[#e06c75]/5 p-3 text-xs text-[#e06c75]">
                <AlertTriangle className="mt-0.5 size-4 shrink-0" />
                <span>{connectionError}</span>
              </div>
            )}
          </div>
          <CardFooter className="justify-between gap-2 border-t border-border/70 pt-4">
            <Button
              type="button"
              variant="ghost"
              onClick={() => {
                setView("list");
                setConnectionError(null);
                setSuffixError(null);
              }}
              disabled={starting}
              className="cursor-pointer text-xs text-foreground hover:bg-secondary"
            >
              Cancelar
            </Button>
            <Button type="submit" disabled={starting} className="cursor-pointer gap-2 text-xs bg-[#61afef] text-primary-foreground hover:bg-[#61afef]/90">
              <ShieldCheck className="size-3.5" />
              {starting ? "Iniciando conexão…" : connectionError ? "Tentar novamente" : `Conectar com ${connectionLabel}`}
            </Button>
          </CardFooter>
        </form>
      </>
    );
  };

  const renderWaiting = () => (
    <>
      <DialogHeader>
        <DialogTitle>Conectar com {connectionLabel}</DialogTitle>
        <DialogDescription>
          A janela de autenticação foi aberta no navegador padrão.
        </DialogDescription>
      </DialogHeader>
      <CardContent className="space-y-4">
        <div className="flex items-center gap-3 rounded-lg border border-[#61afef]/25 bg-[#61afef]/5 p-4" aria-live="polite">
          <Spinner aria-label="Aguardando autenticação no navegador" className="size-5 text-[#61afef]" />
          <div>
            <p className="text-sm font-medium text-foreground">Aguardando autenticação no navegador</p>
          </div>
        </div>
        {connectionError && (
          <p role="alert" className="text-xs text-[#e06c75]">
            {connectionError}
          </p>
        )}
      </CardContent>
      <CardFooter className="flex-col-reverse items-stretch gap-2 border-t border-border/70 pt-4 sm:flex-row sm:justify-end">
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
    </>
  );

  return (
    <>
      <Dialog open={open} onOpenChange={(nextOpen) => handleDialogOpenChange(nextOpen)}>
        <DialogContent
          showCloseButton
          className="settings-panel dark flex max-h-[min(740px,85dvh)] w-[calc(100vw-3rem)] sm:max-w-[860px] flex-col gap-0 border-border bg-background p-0 text-foreground shadow-2xl overflow-hidden motion-reduce:transition-none"
        >
          <DialogHeader className="shrink-0 border-b border-border bg-sidebar px-6 py-5">
            <DialogTitle className="flex items-center gap-3 text-base font-heading font-medium text-foreground"><Settings aria-hidden="true" className="size-4 text-muted-foreground" />Configurações</DialogTitle>
            <DialogDescription className="sr-only">Painel de configurações do Jarvis</DialogDescription>
          </DialogHeader>

          <Tabs value={activeTab} onValueChange={setActiveTab} className="flex flex-col flex-1 min-h-0 gap-0 overflow-hidden">
            <div className="settings-navigation shrink-0 overflow-x-auto border-b border-border bg-sidebar px-6 py-2.5">
              <TabsList aria-label="Configurações" className="w-max gap-1 rounded-md bg-transparent p-0">
                <TabsTrigger value="general" className="cursor-pointer gap-2 px-2 text-xs"><Settings aria-hidden="true" className="size-3.5" />Geral</TabsTrigger>
                <TabsTrigger value="agents" className="cursor-pointer gap-2 px-2 text-xs"><Users aria-hidden="true" className="size-3.5" />Agentes</TabsTrigger>
                <TabsTrigger
                  value="providers"
                  className="cursor-pointer gap-2 px-2 text-xs"
                >
                  <Sparkles aria-hidden="true" className="size-3.5" />
                  <span>Provedores</span>
                  {listState === "ready" && (
                    <Badge className="border-[#61afef]/30 bg-[#61afef]/10 text-[10px] text-[#61afef] px-1.5 py-0">
                      {accounts.length}
                    </Badge>
                  )}
                </TabsTrigger>
                <TabsTrigger value="skills" className="cursor-pointer gap-2 px-2 text-xs"><BookOpen aria-hidden="true" className="size-3.5" />Skills{skillCount !== null && <Badge variant="outline" className="border-[#c678dd]/30 bg-[#c678dd]/10 px-1.5 py-0 text-[10px] text-[#c678dd]">{skillCount}</Badge>}</TabsTrigger>
                <TabsTrigger value="mcps" className="cursor-pointer gap-2 px-2 text-xs"><Plug aria-hidden="true" className="size-3.5" />MCPs{mcpCount !== null && <Badge variant="outline" className="border-[#56b6c2]/30 bg-[#56b6c2]/10 px-1.5 py-0 text-[10px] text-[#56b6c2]">{mcpCount}</Badge>}</TabsTrigger>
              </TabsList>
            </div>

            <TabsContent value="general" className="m-0 min-h-0 flex-1 overflow-y-auto px-6 py-5">{activeTab === "general" && <><CoreSettings /><ChatCleanupSettings /></>}</TabsContent>
            <TabsContent value="agents" className="m-0 min-h-0 flex-1 overflow-y-auto px-6 py-5">{activeTab === "agents" && <AgentSettings accounts={accounts} />}</TabsContent>
            <TabsContent value="skills" className="m-0 min-h-0 flex-1 overflow-y-auto px-6 py-5">{activeTab === "skills" && <SkillsSettings onCountChange={updateSkillCount} />}</TabsContent>
            <TabsContent value="providers" className="flex-1 min-h-0 m-0 overflow-y-auto p-0">
              <div className="px-6 py-5">
                {renderList()}
              </div>
            </TabsContent>
            <TabsContent value="mcps" className="flex-1 min-h-0 m-0 overflow-y-auto p-0">
              <div className="px-6 py-5">{activeTab === "mcps" && <McpSettings onCountChange={updateMcpCount} />}</div>
            </TabsContent>
          </Tabs>
          <Dialog open={open && view !== "list"} onOpenChange={(nextOpen) => { if (!nextOpen && !savingCustom) handleDialogOpenChange(false, false); }}>
            <DialogContent className={`dark max-h-[85vh] overflow-y-auto ${provider === "custom" ? "sm:max-w-2xl" : "sm:max-w-xl"}`}>
              {view === "waiting" ? renderWaiting() : renderAdd()}
            </DialogContent>
          </Dialog>
          <Dialog open={open && editingCustom !== null} onOpenChange={next => { if (!next && !savingCustom) setEditingCustom(null); }}><DialogContent className="dark max-h-[85vh] overflow-y-auto sm:max-w-2xl" aria-describedby={undefined}><DialogHeader><DialogTitle>Editar provedor Custom</DialogTitle></DialogHeader>{editingCustom && <CustomProviderForm key={editingCustom.alias} account={editingCustom} onBusyChange={setSavingCustom} onCancel={() => setEditingCustom(null)} onSaved={customSaved} />}</DialogContent></Dialog>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={disconnectAlias !== null}
        onOpenChange={(nextOpen) => {
          if (!nextOpen && !disconnecting) {
            setDisconnectAlias(null);
            setDisconnectError(null);
          }
        }}
      >
        <AlertDialogContent size="sm" className="dark border-border bg-card text-foreground">
          <AlertDialogHeader>
            <AlertDialogTitle className="text-foreground">Desconectar conta?</AlertDialogTitle>
            <AlertDialogDescription className="text-xs text-foreground">
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
