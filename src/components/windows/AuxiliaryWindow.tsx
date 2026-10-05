import { useCallback, type ReactNode } from "react";
import { emit } from "@tauri-apps/api/event";
import { TextContextMenu } from "@/components/TextContextMenu";
import { Toaster } from "@/components/ui/sonner";
import { SettingsDialog } from "@/components/settings/SettingsDialog";
import { AppUpdate } from "@/components/layout/AppUpdate";
import { PROVIDER_SETTINGS_CHANGED } from "@/core/auxiliary-windows";
import { useAuxiliaryWindowClose } from "@/hooks/use-auxiliary-window-close";

function WindowShell({ children }: { children: ReactNode }) {
  return <TextContextMenu><main className="dark flex h-dvh min-h-0 flex-col overflow-hidden bg-background text-foreground" onContextMenu={event => event.preventDefault()}>{children}<Toaster /></main></TextContextMenu>;
}

export function SettingsWindow() {
  const { close, setBusy, setCloseRequest } = useAuxiliaryWindowClose();
  const accountsChanged = useCallback(() => { void emit(PROVIDER_SETTINGS_CHANGED).catch(() => {}); }, []);
  return <WindowShell><SettingsDialog standalone open onOpenChange={open => { if (!open) { setBusy(false); close(); } }} onBusyChange={setBusy} onCloseRequestChange={setCloseRequest} onAccountsChange={accountsChanged} /></WindowShell>;
}

export function AboutWindow() {
  const { setBusy } = useAuxiliaryWindowClose();
  return <WindowShell><AppUpdate standalone onBusyChange={setBusy} /></WindowShell>;
}
