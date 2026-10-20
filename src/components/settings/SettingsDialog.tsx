import { GlobalDirectorySettings } from "./GlobalDirectorySettings";
import { useCallback, useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { ProviderRemovalDialog } from "./ProviderRemovalDialog";
import type { ProviderRemovalResult } from "@/core/provider-references";
import { compatibleModels } from "@/core/provider-references";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { readResource } from "@/core/resource-request";
import { WebSearchSettings } from "./WebSearchSettings";
import { McpSettings } from "./McpSettings";
import { HooksSettings } from "./HooksSettings";
import { PluginsSettings } from "./PluginsSettings";
import { SkillsSettings } from "./SkillsSettings";
import { CoreSettings } from "./CoreSettings";
import { ComputerUseSettings } from "./ComputerUseSettings";
import { ChatCleanupSettings } from "./ChatCleanupSettings";
import { SystemSettings } from "./SystemSettings";
import { TerminalSettings } from "./TerminalSettings";
import { BrowserSettings } from "./BrowserSettings";
import { VoiceSettings } from "@/components/voice/VoiceSettings";
import { BackupSettings } from "./BackupSettings";
import { WorkspaceSettings } from "./WorkspaceSettings";
import { WorkflowSettings } from "./WorkflowSettings";
import { skillsSnapshotSchema } from "@/core/skills";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  AlertTriangle,
  Anchor,
  Users,
  Layers,
  BookOpen,
  ExternalLink,
  Globe,
  Mic,
  Plus,
  Plug,
  Package,
  Settings,
  ShieldCheck,
  Sparkles,
  Terminal as TerminalIcon,
} from "lucide-react";
import { toast } from "sonner";
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
import { Tabs as SettingsTabs } from "@base-ui/react/tabs";
import { TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { InputGroup, InputGroupAddon, InputGroupText } from "@/components/ui/input-group";
import { InputGroupInput } from "@/components/TextInput";
import { Label } from "@/components/ui/label";
import { CardsSkeleton } from "@/components/layout/LoadingSkeletons";
import { Spinner } from "@/components/ui/spinner";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { accountList, enabledModels, providerDisplayName, providerKindLabels, validateProviderDisplayName, type ProviderAccount, type ProviderUsageAlert } from "@/core/provider-accounts";
import { ProviderDisplayNameField } from "./ProviderDisplayNameField";
import { ProviderAccountCard } from "./ProviderAccountCard";
import { ClaudeProviderCard } from "./ClaudeProviderCard";
import { MuseProviderCard } from "./MuseProviderCard";
import { CustomProviderForm } from "./CustomProviderForm";
import { ApiKeyProviderForm } from "./OpenCodeGoProviderForm";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { useBootstrapResources } from "@/hooks/use-bootstrap-resources";
import { Hint } from "@/components/ui/hint";


const SETTINGS_SECTIONS = [
              { value: "general", label: "Geral", Icon: Settings, iconClassName: "bg-muted-foreground/55", description: "Preferências do aplicativo e organização das conversas." },
              { value: "terminal", label: "Terminal", Icon: TerminalIcon, iconClassName: "bg-secondary", description: "Shell interativo, argumentos e aparência do terminal integrado." },
              { value: "browser", label: "Navegador", Icon: Globe, iconClassName: "bg-primary", description: "Navegação integrada ou controle do seu navegador pela extensão." },
              { value: "voice", label: "Jarvis Voice", Icon: Mic, iconClassName: "bg-onedark-purple/55", description: "Dite mensagens no chat e ajuste a voz das notificações do Jarvito." },
              { value: "workspaces", label: "Espaços", Icon: Layers, iconClassName: "bg-onedark-cyan/55", description: "Projetos, conversas e armazenamento local." },
              { value: "tools", label: "Ferramentas", Icon: Plug, iconClassName: "bg-onedark-yellow/55", description: "Prepare e acompanhe as ferramentas do ambiente." },
              { value: "agents", label: "Fluxos", Icon: Users, iconClassName: "bg-onedark-green/55", description: "Organize fluxos e agentes para o seu jeito de trabalhar." },
              { value: "providers", label: "Provedores", Icon: Sparkles, iconClassName: "bg-primary", description: "Contas, modelos e recursos de inteligência artificial." },
              { value: "skills", label: "Skills", Icon: BookOpen, iconClassName: "bg-onedark-purple/55", description: "Instruções especializadas disponíveis para os agentes." },
              { value: "mcps", label: "MCPs", Icon: Plug, iconClassName: "bg-onedark-cyan/55", description: "Conecte ferramentas e serviços aos seus agentes." },
              { value: "hooks", label: "Hooks", Icon: Anchor, iconClassName: "bg-onedark-yellow/55", description: "Comandos automáticos nos eventos das conversas." },
              { value: "plugins", label: "Plugins", Icon: Package, iconClassName: "bg-onedark-purple/55", description: "Pacotes de skills, MCPs, hooks e apps para os agentes." },
            ];

const ALIAS_SUFFIX_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;


type SettingsView = "list" | "add" | "reauthorize" | "waiting";
type ListState = "loading" | "ready" | "error";

function useCompactSettingsNavigation() {
  const [compact, setCompact] = useState(false);
  useEffect(() => {
    const media = window.matchMedia("(max-width: 639px)");
    const update = () => setCompact(media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  return compact;
}


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
  initialSection?: "computer";
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAccountsChange?: (accounts: ProviderAccount[]) => void;
  embeddedProviders?: boolean;
  standalone?: boolean;
  onBusyChange?: (busy: boolean) => void;
  onCloseRequestChange?: (handler: (() => void) | null) => void;
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


function SettingsSurface({ embedded, standalone, open, onOpenChange, children }: { embedded: boolean; standalone: boolean; open: boolean; onOpenChange: (open: boolean) => void; children: ReactNode }) {
  if (embedded) return <div className="min-w-0">{children}</div>;
  if (standalone) return <Card role="region" aria-label="Configurações" className="settings-panel flex h-full min-h-0 w-full flex-1 flex-col gap-0 overflow-hidden rounded-none border-0 bg-background py-0 shadow-none">{children}</Card>;
  return <Dialog open={open} onOpenChange={(next, details) => { if (details.reason !== "outside-press" && details.reason !== "escape-key") onOpenChange(next); }}><DialogContent showCloseButton className="settings-panel dark flex h-[min(960px,94dvh)] max-h-[94dvh] w-[calc(100vw-2rem)] max-w-none sm:max-w-[1400px] flex-col gap-0 border-border bg-background p-0 text-foreground shadow-2xl overflow-hidden motion-reduce:transition-none">{children}</DialogContent></Dialog>;
}

function SettingsPage({ value, children }: { value: string; children: ReactNode }) {
  return <TabsContent value={value} className="settings-scroll-pane m-0 min-h-0 min-w-0 w-full overflow-y-auto"><div className="settings-page p-4 sm:p-6">{children}</div></TabsContent>;
}

export function SettingsDialog(props: SettingsDialogProps) {
  return props.open ? <SettingsSession {...props} /> : null;
}

function SettingsSession({ open, onOpenChange, onAccountsChange, embeddedProviders = false, standalone = false, onBusyChange, onCloseRequestChange, initialSection }: SettingsDialogProps) {
  const compactSettingsNavigation = useCompactSettingsNavigation();
  const bootstrap = useBootstrapResources();
  const [activeTab, setTab] = useState(initialSection === "computer" || window.location.hash === "#computer" ? "tools" : "general");
  const [computerNavigation, setComputerNavigation] = useState(initialSection === "computer" || window.location.hash === "#computer" ? 1 : 0);
  useEffect(() => {
    const subscription = listen("settings:navigate", event => {
      if (event.payload === "computer") { setTab("tools"); setComputerNavigation(value => value + 1); }
    });
    return () => { void subscription.then(stop => stop()).catch(() => {}); };
  }, []);
  useEffect(() => {
    if (activeTab !== "tools" || !computerNavigation) return;
    const section = document.getElementById("computer-use-settings");
    section?.scrollIntoView({ block: "start" });
    section?.focus({ preventScroll: true });
  }, [activeTab, computerNavigation]);
  const [workflowInitialTab, setWorkflowInitialTab] = useState<"flows" | "agents">("flows");
  const setActiveTab = (value: string) => {
    if (value !== "general" && value !== "terminal" && value !== "browser" && value !== "voice" && value !== "tools" && value !== "providers" && value !== "agents" && value !== "skills" && value !== "mcps" && value !== "hooks" && value !== "plugins" && value !== "workspaces") return;
    setTab(value);
  };
  const [mcpCount, setMcpCount] = useState<number | null>(null);
  const [skillCount, setSkillCount] = useState<number | null>(() => bootstrap?.resources.skills?.skills.length ?? null);
  const visibleSkillCount = skillCount ?? bootstrap?.resources.skills?.skills.length ?? null;
  const skillCountVersion = useRef(0);
  const updateSkillCount = useCallback((count: number) => { skillCountVersion.current += 1; setSkillCount(count); }, []);
  const mcpCountVersion = useRef(0);
  const updateMcpCount = useCallback((count: number) => {
    mcpCountVersion.current += 1;
    setMcpCount(count);
  }, []);
  const [view, setView] = useState<SettingsView>("list");
  const [listState, setListState] = useState<ListState>(() => bootstrap?.resources.loaded.accounts ? "ready" : "loading");
  const [accounts, setAccounts] = useState<ProviderAccount[]>(() => bootstrap?.resources.accounts ?? []);
  const [listError, setListError] = useState<string | null>(null);
  const [suffix, setSuffix] = useState("");
  const [displayName, setDisplayName] = useState<string | null>(null);
  const [provider, setProvider] = useState("openai-codex");
  const [editingCustom, setEditingCustom] = useState<ProviderAccount | null>(null);
  const [reauthorizing, setReauthorizing] = useState<ProviderAccount | null>(null);
  const [savingCustom, setSavingCustom] = useState(false);
  const aliasPrefix = `${provider}-`;
  const connectionLabel = provider === "antigravity" ? "Antigravity" : "ChatGPT";
  const [suffixError, setSuffixError] = useState<string | null>(null);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [disconnectAlias, setDisconnectAlias] = useState<string | null>(null);
  const [disconnecting, setDisconnecting] = useState<string | null>(null);
  const [toggling, setToggling] = useState(false);
  const [refreshingAlias, setRefreshingAlias] = useState<string | null>(null);
  const togglingRef = useRef(false);

  const activeConnectionRef = useRef<ActiveConnection | null>(null);
  const listRequestRef = useRef(0);
  const startingRef = useRef(false);
  const cancellingRef = useRef(false);
  const closeRequestedRef = useRef(false);
  const closingRef = useRef(false);
  const preloadedAccounts = useRef(bootstrap?.resources.loaded.accounts ?? false);
  const [searchBusy, setSearchBusy] = useState(false);
  const [visionBusy, setVisionBusy] = useState(false);
  const [hooksBusy, setHooksBusy] = useState(false);
  const [pluginsBusy, setPluginsBusy] = useState(false);
  const operationBusy = hooksBusy || pluginsBusy || savingCustom || (starting && view !== "waiting") || cancelling || toggling || refreshingAlias !== null || disconnecting !== null || searchBusy || visionBusy;
  useEffect(() => { onBusyChange?.(standalone ? operationBusy : operationBusy || view !== "list" || editingCustom !== null || listState !== "ready"); }, [view, editingCustom, listState, operationBusy, standalone, onBusyChange]);
  useEffect(() => {
    closeRequestedRef.current = false;
    return () => {
      listRequestRef.current += 1;
      closeRequestedRef.current = true;
      const connection = activeConnectionRef.current;
      activeConnectionRef.current = null;
      if (connection) void invoke("cancel_openai_codex_connection", { flowId: connection.flowId }).catch(() => {});
    };
  }, []);

  const updateAccounts = useCallback(
    (result: ProviderAccount[] | undefined) => {
      const nextAccounts = accountList(result);
      setAccounts(nextAccounts);
      onAccountsChange?.(nextAccounts);
      setListState("ready");
    },
    [onAccountsChange],
  );

  const loadAccounts = useCallback(async (cached = false) => {
    const requestId = ++listRequestRef.current;
    try {
      const result = await readResource<ProviderAccount[]>("list_provider_accounts", cached ? { cached: true } : undefined);
      if (requestId !== listRequestRef.current) return;
      updateAccounts(result);
    } catch (error) {
      if (requestId !== listRequestRef.current) return;
      setListError(safeErrorMessage(error, "Não foi possível acessar as contas conectadas."));
      setListState("error");
    }
  }, [updateAccounts]);
  const refreshImageAccounts = useCallback(() => loadAccounts(true), [loadAccounts]);

  useEffect(() => {
    if (!open) return;
    if (preloadedAccounts.current) {
      preloadedAccounts.current = false;
      return;
    }
    const requestId = ++listRequestRef.current;
    void readResource<ProviderAccount[]>("list_provider_accounts").then(
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
    if (!open || embeddedProviders) return;
    let active = true;
    let dispose: (() => void) | undefined;
    const refresh = () => {
      if (!active) return;
      const version = ++mcpCountVersion.current;
      void invoke<unknown>("list_mcp_servers").then((value) => {
        if (active && version === mcpCountVersion.current) setMcpCount(mcpServersSchema.parse(value).length);
      }).catch(() => { if (active && version === mcpCountVersion.current) setMcpCount(null); });
    };
    void listen("mcp-servers:changed", refresh).then(stop => {
      if (active) { dispose = stop; refresh(); } else stop();
    }).catch(refresh);
    return () => { active = false; dispose?.(); };
  }, [open, embeddedProviders]);

  useEffect(() => {
    if (!open || embeddedProviders) return;
    let active = true;
    const version = skillCountVersion.current;
    void invoke("list_skills").then(value => {
      if (active && version === skillCountVersion.current) setSkillCount(skillsSnapshotSchema.parse(value).skills.length);
    }).catch(() => { if (active && version === skillCountVersion.current) setSkillCount(null); });
    return () => { active = false; };
  }, [open, embeddedProviders]);

  const openAddView = () => {
    if (startingRef.current || cancellingRef.current) return;
    setReauthorizing(null);
    setDisplayName(null);
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

  const connectAccount = async (alias: string, reauthorize = false) => {
    if (startingRef.current || activeConnectionRef.current) return;
    startingRef.current = true;
    setStarting(true);
    setConnectionError(null);

    let started: ActiveConnection | null = null;
    try {
      const result = await invoke<ConnectionStart>(reauthorize ? "reauthorize_provider_account" : "begin_openai_codex_connection", { alias, ...(!reauthorize ? { displayName: (displayName ?? alias).trim() } : {}) });
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
          setView(reauthorize ? "reauthorize" : "add");
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
      toast.success(reauthorize ? "Autorização atualizada" : "Conta conectada");
    } catch (error) {
      if (!started) {
        setConnectionError(safeErrorMessage(error, "Não foi possível iniciar a conexão."));
      } else if (activeConnectionRef.current?.flowId === started.flowId) {
        clearActiveConnection();
        setView(reauthorize ? "reauthorize" : "add");
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

  const handleConnect = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const validationError = validateAliasSuffix(suffix);
    setSuffixError(validationError);
    if (validationError) return;
    const nameError = validateProviderDisplayName(displayName ?? `${aliasPrefix}${suffix}`);
    setConnectionError(nameError);
    if (!nameError) void connectAccount(`${aliasPrefix}${suffix}`);
  };

  const handleReauthorize = (account: ProviderAccount) => {
    if (startingRef.current || cancellingRef.current) return;
    setReauthorizing(account);
    setProvider(account.providerKind);
    setConnectionError(null);
    setView("reauthorize");
    void connectAccount(account.alias, true);
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

  const handleDialogOpenChange = useCallback((nextOpen: boolean, closeSettings = true) => {
    if ((savingCustom || hooksBusy || pluginsBusy) && !nextOpen) return;
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
  }, [savingCustom, hooksBusy, pluginsBusy, onOpenChange, cancelActiveConnection]);

  useEffect(() => {
    onCloseRequestChange?.(() => handleDialogOpenChange(false));
    return () => onCloseRequestChange?.(null);
  }, [handleDialogOpenChange, onCloseRequestChange]);

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

  const handleProviderRemoved = async (result: ProviderRemovalResult) => {
    updateAccounts(accounts.filter(account => account.alias !== disconnectAlias));
    setDisconnectAlias(null);
    setListState("loading");
    setListError(null);
    toast.success(result.replaced ? `Provedor removido e ${result.replaced} vínculos atualizados` : "Provedor removido");
    if (result.unresolved.length) toast.error(`${result.unresolved.length} vínculos ficaram sem provedor`, { id: "provider-invalid-models", description: result.unresolved.map(item => item.label).join(", "), duration: 10000 });
    await loadAccounts();
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

  const handleDisplayNameChange = async (alias: string, displayName: string) => {
    if (togglingRef.current) return;
    togglingRef.current = true; setToggling(true);
    try {
      await invoke("set_provider_display_name", { alias, displayName });
      updateAccounts(accounts.map(account => account.alias === alias ? { ...account, displayName } : account));
      toast.success("Nome do provedor atualizado");
    } catch { toast.error("Não foi possível salvar o nome do provedor."); }
    finally { togglingRef.current = false; setToggling(false); }
  };

  const handleModelEnabledChange = async (alias: string, modelId: string, enabled: boolean) => {
    if (togglingRef.current) return;
    togglingRef.current = true; setToggling(true);
    try {
      const disabledModels = await invoke<string[]>("set_provider_model_enabled", { alias, modelId, enabled });
      if (!Array.isArray(disabledModels) || !disabledModels.every(id => typeof id === "string")) throw new Error("invalid response");
      updateAccounts(accounts.map(account => account.alias === alias ? { ...account, disabledModels } : account));
      toast.success(enabled ? "Modelo disponibilizado" : "Modelo oculto da seleção");
    } catch { toast.error("Não foi possível salvar a disponibilidade do modelo."); }
    finally { togglingRef.current = false; setToggling(false); }
  };

  const handleRefreshModels = async (alias: string) => {
    if (refreshingAlias !== null || togglingRef.current) return;
    setRefreshingAlias(alias);
    try {
      const fetched = accountList(await readResource("refresh_provider_models", { alias }));
      const refreshed = fetched.find(account => account.alias === alias);
      if (!refreshed || !refreshed.modelsAvailable) {
        toast.error("Não foi possível atualizar os modelos", { description: "O catálogo anterior foi mantido. Confira a conexão e tente novamente." });
        return;
      }
      const previous = accounts.find(account => account.alias === alias);
      const removed = previous?.models.filter(model => !refreshed.models.some(next => next.id === model.id)) ?? [];
      updateAccounts(accounts.map(account => account.alias === alias ? refreshed : account));
      toast.success("Modelos atualizados", { description: removed.length ? `${removed.length} modelo(s) saíram do catálogo. Revise os agentes vinculados abaixo.` : `${refreshed.models.length} modelo(s) encontrados.` });
    } catch (error) {
      toast.error("Não foi possível atualizar os modelos", { description: safeErrorMessage(error, "O catálogo anterior foi mantido.") });
    } finally { setRefreshingAlias(null); }
  };

  const handleUsageChange = async (alias: string, showUsage: boolean, showThirdPartyUsage: boolean, showFiveHourUsage?: boolean, showWeeklyUsage?: boolean) => {
    if (togglingRef.current) return;
    togglingRef.current = true; setToggling(true);
    try {
      const windows = {
        ...(showFiveHourUsage === undefined ? {} : { showFiveHourUsage }),
        ...(showWeeklyUsage === undefined ? {} : { showWeeklyUsage }),
      };
      await invoke("set_provider_usage_visibility", { alias, showUsage, showThirdPartyUsage, ...windows });
      updateAccounts(accounts.map(account => account.alias === alias ? { ...account, showUsage, showThirdPartyUsage, ...windows } : account));
    } catch { toast.error("Não foi possível salvar a visualização dos limites."); }
    finally { togglingRef.current = false; setToggling(false); }
  };

  const handleUsageAlertChange = async (alias: string, alert: ProviderUsageAlert | null) => {
    if (togglingRef.current) return;
    togglingRef.current = true; setToggling(true);
    try {
      await invoke("set_provider_usage_alert", { alias, alert });
      updateAccounts(accounts.map(account => account.alias === alias ? { ...account, usageAlert: alert } : account));
    } catch { toast.error("Não foi possível salvar o alerta de limite."); }
    finally { togglingRef.current = false; setToggling(false); }
  };

  const renderList = () => {
    if (listState === "loading") {
      return (
        <CardsSkeleton label="Carregando contas conectadas" />
      );
    }

    if (listState === "error") {
      return (
        <Card className="border-destructive/40 bg-destructive/5" role="alert">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-sm text-destructive">
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
              className="cursor-pointer border-destructive/40 text-destructive hover:bg-destructive/10"
            >
              Tentar novamente
            </Button>
          </CardFooter>
        </Card>
      );
    }

    return (
      <div className="flex min-w-0 flex-col gap-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="settings-section-title text-muted-foreground">Provedores</h2>
          </div>
            <Button
              type="button"
              onClick={openAddView}
              className="cursor-pointer gap-1.5 bg-primary text-xs font-medium text-primary-foreground hover:bg-primary/90"
            >
              <Plus className="size-3.5" />
              Adicionar conta
            </Button>
        </div>

        <div className="flex min-w-0 flex-col gap-3">
            <ClaudeProviderCard />
            <MuseProviderCard />
            {accounts.map((account) => (
              <ProviderAccountCard
                key={account.alias}
                account={account}
                saving={toggling || starting || cancelling || refreshingAlias !== null}
                refreshing={refreshingAlias === account.alias}
                onModelEnabledChange={(alias, modelId, enabled) => { void handleModelEnabledChange(alias, modelId, enabled); }}
                onRefreshModels={(alias) => { void handleRefreshModels(alias); }}
                onReviewAgents={embeddedProviders ? undefined : tab => { setWorkflowInitialTab(tab); setActiveTab("agents"); }}
                onEdit={setEditingCustom}
                onReauthorize={handleReauthorize}
                onUsageChange={(alias, showUsage, showThirdPartyUsage, showFiveHourUsage, showWeeklyUsage) => { void handleUsageChange(alias, showUsage, showThirdPartyUsage, showFiveHourUsage, showWeeklyUsage); }}
                onUsageAlertChange={(alias, alert) => { void handleUsageAlertChange(alias, alert); }}
                onEnabledChange={(alias, enabled) => { void handleEnabledChange(alias, enabled); }}
                onDisplayNameChange={(alias, displayName) => { void handleDisplayNameChange(alias, displayName); }}
                onDisconnect={(alias) => {
                  setDisconnectAlias(alias);
                }}
              />
            ))}
          </div>
        {(!embeddedProviders || accounts.some(account => account.enabled && ((account.modelsAvailable && enabledModels(account).length > 0) || compatibleModels(account, "image_generation").length > 0))) && <section className="space-y-3" aria-label="Ferramentas"><h3 className="settings-section-title text-muted-foreground">Ferramentas</h3><div className="settings-card-grid"><WebSearchSettings accounts={accounts} onBusyChange={embeddedProviders ? setSearchBusy : undefined} /><WebSearchSettings accounts={accounts} kind="vision" onBusyChange={embeddedProviders ? setVisionBusy : undefined} /><WebSearchSettings accounts={accounts} kind="image_generation" onImageCatalogRefresh={refreshImageAccounts} /></div></section>}
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
    const providerSelect = <FieldGroup><Field><FieldLabel htmlFor="account-provider">Provedor</FieldLabel><Select value={provider} onValueChange={value => { if (value) { setProvider(value); setConnectionError(null); } }} disabled={starting || savingCustom}><SelectTrigger id="account-provider" className="w-full cursor-pointer"><SelectValue>{providerKindLabels[provider] ?? provider}</SelectValue></SelectTrigger><SelectContent><SelectGroup>{["openai-codex", "antigravity", "opencode-go", "charm-hyper", "openrouter", "ollama", "brave-search", "custom"].map(kind => <SelectItem key={kind} value={kind} className="cursor-pointer">{providerKindLabels[kind]}</SelectItem>)}</SelectGroup></SelectContent></Select></Field></FieldGroup>;
    if (provider === "opencode-go" || provider === "charm-hyper" || provider === "brave-search") return <><DialogHeader><DialogTitle>Adicionar {providerKindLabels[provider]}</DialogTitle><DialogDescription>Conecte sua conta usando uma chave de API.</DialogDescription></DialogHeader>{providerSelect}<ApiKeyProviderForm key={provider} providerKind={provider} onBusyChange={setSavingCustom} onCancel={() => setView("list")} onSaved={customSaved} /></>;
    if (provider === "openrouter") return <><DialogHeader><DialogTitle>Adicionar OpenRouter</DialogTitle><DialogDescription>Conecte sua chave e adicione os modelos que deseja usar.</DialogDescription></DialogHeader>{providerSelect}<CustomProviderForm providerKind="openrouter" onBusyChange={setSavingCustom} onCancel={() => setView("list")} onSaved={customSaved} /></>;
    if (provider === "ollama") return <><DialogHeader><DialogTitle>Adicionar Ollama</DialogTitle><DialogDescription>Informe o servidor e cadastre apenas os modelos que deseja usar.</DialogDescription></DialogHeader>{providerSelect}<CustomProviderForm key={provider} providerKind="ollama" onBusyChange={setSavingCustom} onCancel={() => setView("list")} onSaved={customSaved} /></>;
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
                  <InputGroupText className="font-mono text-xs text-onedark-cyan">
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
                <p id="provider-alias-error" role="alert" className="text-xs text-destructive">
                  {suffixError}
                </p>
              )}
              <p className="text-xs text-foreground">
                Alias completo: <code className="font-mono text-primary">{computedAlias}</code>
              </p>
            </div>
            <ProviderDisplayNameField value={displayName ?? computedAlias} onChange={setDisplayName} disabled={starting} />
            {connectionError && (
              <div role="alert" className="flex items-start gap-2 rounded-lg border border-destructive/35 bg-destructive/5 p-3 text-xs text-destructive">
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
            <Button type="submit" disabled={starting} className="cursor-pointer gap-2 text-xs bg-primary text-primary-foreground hover:bg-primary/90">
              <ShieldCheck className="size-3.5" />
              {starting ? "Iniciando conexão…" : connectionError ? "Tentar novamente" : `Conectar com ${connectionLabel}`}
            </Button>
          </CardFooter>
        </form>
      </>
    );
  };

  const renderReauthorize = () => (
    <>
      <DialogHeader>
        <DialogTitle>Re-autorizar provedor</DialogTitle>
        <DialogDescription>
          Entre novamente com {connectionLabel} para atualizar a autorização de <span>{reauthorizing ? providerDisplayName(reauthorizing) : ""}</span>. O alias e as configurações serão mantidos.
        </DialogDescription>
      </DialogHeader>
      {connectionError && <p role="alert" className="text-xs text-destructive">{connectionError}</p>}
      <CardFooter className="justify-end gap-2 border-t border-border pt-4">
        <Button variant="ghost" className="cursor-pointer" disabled={starting} onClick={() => setView("list")}>Voltar</Button>
        <Button className="cursor-pointer" disabled={starting} onClick={() => { if (reauthorizing) void connectAccount(reauthorizing.alias, true); }}>
          {starting ? "Iniciando autorização…" : "Tentar novamente"}
        </Button>
      </CardFooter>
    </>
  );

  const renderWaiting = () => (
    <>
      <DialogHeader>
        <DialogTitle>{reauthorizing ? "Re-autorizar" : "Conectar com"} {connectionLabel}</DialogTitle>
        <DialogDescription>
          A janela de autenticação foi aberta no navegador padrão.{reauthorizing && " O alias e as configurações serão mantidos."}
        </DialogDescription>
      </DialogHeader>
      <CardContent className="space-y-4">
        <div className="flex items-center gap-3 rounded-lg border border-primary/25 bg-primary/5 p-4" aria-live="polite">
          <Spinner aria-label="Aguardando autenticação no navegador" className="size-5 text-primary" />
          <div>
            <p className="text-sm font-medium text-foreground">Aguardando autenticação no navegador</p>
          </div>
        </div>
        {connectionError && (
          <p role="alert" className="text-xs text-destructive">
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
          className="cursor-pointer border-destructive/40 text-xs text-destructive hover:bg-destructive/10"
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
      <SettingsSurface embedded={embeddedProviders} standalone={standalone} open={open} onOpenChange={(nextOpen) => handleDialogOpenChange(nextOpen)}>
          {embeddedProviders ? renderList() : <>{!standalone && <DialogHeader className="shrink-0 border-b border-border bg-sidebar px-5 py-4">
            <DialogTitle className="flex items-center gap-3 text-base font-heading font-medium text-foreground"><Settings aria-hidden="true" className="size-4 text-muted-foreground" />Configurações</DialogTitle>
            <DialogDescription className="sr-only">Painel de configurações do Jarvis</DialogDescription>
          </DialogHeader>}

          <SettingsTabs.Root orientation="vertical" value={activeTab} onValueChange={value => { if (typeof value === "string") setActiveTab(value); }} className="group/tabs flex min-h-0 min-w-0 flex-1 gap-0 overflow-hidden">
              <div className="settings-navigation w-14 shrink-0 overflow-y-auto border-r border-border bg-sidebar p-2 sm:w-60 sm:p-3">
              <TabsList aria-label="Configurações" className="h-auto w-full flex-col items-stretch justify-start gap-1 rounded-none bg-transparent p-0">
                  {SETTINGS_SECTIONS.map(section => <Hint key={section.value} content={section.label} disabled={!compactSettingsNavigation}><TabsTrigger value={section.value} disabled={hooksBusy || pluginsBusy} className="h-9 flex-none cursor-pointer justify-center gap-2.5 px-3 text-sm sm:justify-start">
                    <span className={`flex size-6 shrink-0 items-center justify-center rounded-md text-primary-foreground ${section.iconClassName}`}><section.Icon aria-hidden="true" className="size-4" /></span>
                  <span className="sr-only min-w-0 flex-1 text-left sm:not-sr-only">{section.label}</span>
                  <span className="hidden font-mono text-[10px] text-muted-foreground sm:inline">{section.value === "providers" && listState === "ready" ? accounts.length + 1 : section.value === "skills" ? visibleSkillCount : section.value === "mcps" ? mcpCount : null}</span>
                </TabsTrigger></Hint>)}
              </TabsList>
            </div>
            <div className="flex min-h-0 min-w-0 flex-1 flex-col">
                <div className="shrink-0 border-b border-border px-4 py-5 sm:px-6">
                  <h2 className="text-lg font-semibold">{SETTINGS_SECTIONS.find(section => section.value === activeTab)?.label}</h2>
                <p className="mt-1 text-xs text-muted-foreground">{SETTINGS_SECTIONS.find(section => section.value === activeTab)?.description}</p>
              </div>
                <SettingsPage value="general">{activeTab === "general" && <div className="w-full space-y-6"><SystemSettings accounts={accounts} /><GlobalDirectorySettings /><ChatCleanupSettings /><BackupSettings accounts={accounts} onRestored={summary => { updateSkillCount(summary.skills); updateMcpCount(summary.mcps); }} /></div>}</SettingsPage>
                <SettingsPage value="terminal">{activeTab === "terminal" && <TerminalSettings />}</SettingsPage>
                <SettingsPage value="browser">{activeTab === "browser" && <BrowserSettings />}</SettingsPage>
                <SettingsPage value="voice">{activeTab === "voice" && <VoiceSettings />}</SettingsPage>
                <SettingsPage value="workspaces">{activeTab === "workspaces" && <WorkspaceSettings />}</SettingsPage>
                <SettingsPage value="tools">{activeTab === "tools" && <div className="space-y-6"><ComputerUseSettings /><CoreSettings /></div>}</SettingsPage>
                <SettingsPage value="agents">{activeTab === "agents" && <WorkflowSettings accounts={accounts} initialTab={workflowInitialTab} />}</SettingsPage>
                <SettingsPage value="skills">{activeTab === "skills" && <SkillsSettings onCountChange={updateSkillCount} />}</SettingsPage>
                <SettingsPage value="providers">{renderList()}</SettingsPage>
                <SettingsPage value="mcps">{activeTab === "mcps" && <div className="w-full"><McpSettings onCountChange={updateMcpCount} /></div>}</SettingsPage>
                <SettingsPage value="hooks">{activeTab === "hooks" && <HooksSettings onBusyChange={setHooksBusy} />}</SettingsPage>
                <SettingsPage value="plugins">{activeTab === "plugins" && <PluginsSettings accounts={accounts} onBusyChange={setPluginsBusy} />}</SettingsPage>
            </div>
          </SettingsTabs.Root></>}
          <Dialog open={open && view !== "list"} onOpenChange={(nextOpen) => { if (!nextOpen && !savingCustom) handleDialogOpenChange(false, false); }}>
            <DialogContent className={`dark max-h-[85vh] grid-cols-1 overflow-x-hidden overflow-y-auto wrap-anywhere [scrollbar-gutter:stable] ${(provider === "custom" || provider === "openrouter" || provider === "ollama") ? "sm:max-w-2xl" : "sm:max-w-xl"}`}>
              {view === "waiting" ? renderWaiting() : view === "reauthorize" ? renderReauthorize() : renderAdd()}
            </DialogContent>
          </Dialog>
          <Dialog open={open && editingCustom !== null} onOpenChange={next => { if (!next && !savingCustom) setEditingCustom(null); }}><DialogContent className={`dark max-h-[85vh] grid-cols-1 overflow-x-hidden overflow-y-auto wrap-anywhere [scrollbar-gutter:stable] ${(editingCustom?.providerKind === "custom" || editingCustom?.providerKind === "openrouter" || editingCustom?.providerKind === "ollama") ? "sm:max-w-2xl" : "sm:max-w-xl"}`} aria-describedby={undefined}><DialogHeader className="pr-6"><DialogTitle>{editingCustom?.providerKind === "custom" ? "Editar provedor Custom" : `Editar ${providerKindLabels[editingCustom?.providerKind ?? ""]}`}</DialogTitle></DialogHeader>{editingCustom && (editingCustom.providerKind === "opencode-go" || editingCustom.providerKind === "charm-hyper" || editingCustom.providerKind === "brave-search" ? <ApiKeyProviderForm key={editingCustom.alias} account={editingCustom} providerKind={editingCustom.providerKind} onBusyChange={setSavingCustom} onCancel={() => setEditingCustom(null)} onSaved={customSaved} /> : <CustomProviderForm key={editingCustom.alias} account={editingCustom} onBusyChange={setSavingCustom} onCancel={() => setEditingCustom(null)} onSaved={customSaved} />)}</DialogContent></Dialog>
      </SettingsSurface>

      {disconnectAlias && <ProviderRemovalDialog key={disconnectAlias} alias={disconnectAlias} accounts={accounts} onClose={() => setDisconnectAlias(null)} onBusyChange={busy => setDisconnecting(busy ? disconnectAlias : null)} onRemoved={handleProviderRemoved} />}
    </>
  );
}

export default SettingsDialog;
