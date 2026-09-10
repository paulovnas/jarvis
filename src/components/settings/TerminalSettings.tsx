import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CircleHelp, Save, Sparkles, Terminal as TerminalIcon, Type } from "lucide-react";
import { toast } from "sonner";
import { Input, Textarea } from "@/components/TextInput";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { libraryError } from "@/core/library";
import {
  DEFAULT_TERMINAL_PREFERENCES,
  resolveTerminalFont,
  systemSnapshotSchema,
  terminalFontFamily,
  type SystemSnapshot,
  type TerminalPreferences,
} from "@/core/system-preferences";

const CUSTOM_VALUE = "__custom__";
const AUTOMATIC_VALUE = "__automatic__";

const terminalError = (cause: unknown, fallback: string) => typeof cause === "string" ? cause : libraryError(cause, fallback);

type Draft = {
  shellChoice: string;
  customShell: string;
  arguments: string;
  fontChoice: string;
  customFont: string;
  fontSize: string;
};

function shellName(path: string) {
  return path.split(/[\\/]/).pop()?.replace(/\.exe$/i, "") || path;
}

function createDraft(snapshot: SystemSnapshot): Draft {
  const preferences = snapshot.preferences.terminal;
  const configuredShell = preferences.shell;
  const knownShell = configuredShell && snapshot.availableTerminalShells.includes(configuredShell);
  const configuredFont = preferences.fontFamily;
  const knownFont = configuredFont && snapshot.availableTerminalFonts.find(font => font.toLowerCase() === configuredFont.toLowerCase());
  return {
    shellChoice: configuredShell ? knownShell ? configuredShell : CUSTOM_VALUE : AUTOMATIC_VALUE,
    customShell: configuredShell && !knownShell ? configuredShell : "",
    arguments: preferences.arguments.join("\n"),
    fontChoice: configuredFont ? knownFont ?? CUSTOM_VALUE : AUTOMATIC_VALUE,
    customFont: configuredFont && !knownFont ? configuredFont : "",
    fontSize: String(preferences.fontSize),
  };
}

function draftPreferences(draft: Draft): TerminalPreferences | string {
  const shell = draft.shellChoice === AUTOMATIC_VALUE
    ? null
    : draft.shellChoice === CUSTOM_VALUE
      ? draft.customShell.trim()
      : draft.shellChoice;
  if (shell !== null && !shell) return "Informe o caminho ou nome do shell personalizado.";
  const argumentsList = draft.arguments.split("\n").map(value => value.trim()).filter(Boolean);
  if (argumentsList.length > 16 || argumentsList.some(value => value.length > 512)) {
    return "Use até 16 argumentos, com no máximo 512 caracteres em cada linha.";
  }
  const fontFamily = draft.fontChoice === AUTOMATIC_VALUE
    ? null
    : draft.fontChoice === CUSTOM_VALUE
      ? draft.customFont.trim()
      : draft.fontChoice;
  if (fontFamily !== null && !fontFamily) return "Informe o nome da família de fonte personalizada.";
  const fontSize = Number(draft.fontSize);
  if (!Number.isInteger(fontSize) || fontSize < 9 || fontSize > 32) {
    return "O tamanho da fonte deve ficar entre 9 e 32 pixels.";
  }
  return { shell, arguments: argumentsList, fontFamily, fontSize };
}

function Help({ label, children }: { label: string; children: string }) {
  return <Tooltip><TooltipTrigger render={<Button type="button" variant="ghost" size="icon-sm" />} aria-label={`Ajuda: ${label}`} className="size-5 shrink-0 cursor-pointer text-muted-foreground"><CircleHelp className="size-3" /></TooltipTrigger><TooltipContent className="max-w-72 text-xs leading-5">{children}</TooltipContent></Tooltip>;
}

export function TerminalSettings() {
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const dirty = useRef(false);

  const applySnapshot = useCallback((next: SystemSnapshot, force = false) => {
    setSnapshot(next);
    if (force || !dirty.current) setDraft(createDraft(next));
  }, []);

  useEffect(() => {
    let alive = true;
    let changed = false;
    let unlisten: (() => void) | undefined;
    const start = async () => {
      try {
        const stop = await listen("system:changed", event => {
          const parsed = systemSnapshotSchema.safeParse(event.payload);
          if (alive && parsed.success) {
            changed = true;
            applySnapshot(parsed.data);
          }
        });
        if (!alive) {
          stop();
          return;
        }
        unlisten = stop;
        const initial = systemSnapshotSchema.parse(await invoke("get_system_preferences"));
        if (alive && !changed) applySnapshot(initial, true);
      } catch (cause) {
        if (alive) setError(terminalError(cause, "Não foi possível carregar as preferências do terminal."));
      }
    };
    void start();
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [applySnapshot, attempt]);

  const patchDraft = (patch: Partial<Draft>) => {
    dirty.current = true;
    setError(null);
    setDraft(current => current ? { ...current, ...patch } : current);
  };

  const save = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!snapshot || !draft || busy) return;
    const preferences = draftPreferences(draft);
    if (typeof preferences === "string") {
      setError(preferences);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const next = systemSnapshotSchema.parse(await invoke("save_system_preferences", {
        preferences: { ...snapshot.preferences, terminal: preferences },
      }));
      dirty.current = false;
      applySnapshot(next, true);
      toast.success("Preferências do terminal salvas", {
        description: "A aparência foi atualizada. Shell e argumentos serão usados nos próximos terminais.",
      });
    } catch (cause) {
      setError(terminalError(cause, "Não foi possível salvar as preferências do terminal."));
    } finally {
      setBusy(false);
    }
  };

  if (!snapshot || !draft) {
    return error
      ? <div className="space-y-3"><p role="alert" className="text-xs text-destructive">{error}</p><Button type="button" size="sm" variant="outline" className="cursor-pointer" onClick={() => { setError(null); setAttempt(value => value + 1); }}>Tentar novamente</Button></div>
      : <div role="status" aria-label="Carregando preferências do terminal" className="grid gap-4 lg:grid-cols-2"><Skeleton className="h-64" /><Skeleton className="h-64" /></div>;
  }

  const proposed = draftPreferences(draft);
  const validationError = typeof proposed === "string" ? proposed : null;
  const preview: TerminalPreferences = typeof proposed === "string" ? DEFAULT_TERMINAL_PREFERENCES : proposed;
  const changed = !validationError && JSON.stringify(proposed) !== JSON.stringify(snapshot.preferences.terminal);
  const automaticLabel = snapshot.resolvedTerminalShell
    ? `Automático · ${shellName(snapshot.resolvedTerminalShell)}`
    : "Automático";
  const automaticFont = resolveTerminalFont(null, snapshot.availableTerminalFonts) ?? "JetBrains Mono";
  const fontWarning = typeof proposed !== "string" && proposed.fontFamily
    && !snapshot.availableTerminalFonts.some(font => font.toLowerCase() === proposed.fontFamily?.toLowerCase())
    ? `A fonte '${proposed.fontFamily}' não foi encontrada no sistema. Instale-a ou escolha uma das fontes detectadas.`
    : !changed ? snapshot.terminalFontError : null;

  return <TooltipProvider><form className="space-y-5" onSubmit={save}>
    <div className="grid gap-4 lg:grid-cols-2">
      <Card className="min-w-0 gap-5 p-5">
        <div>
          <h3 className="flex items-center gap-2 text-sm font-medium"><TerminalIcon className="size-4 text-onedark-green" />Shell interativo</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">Escolha o ambiente usado nas abas de terminal abertas no Jarvis.</p>
        </div>
        <div className="space-y-2">
          <div className="flex items-center gap-1"><Label htmlFor="terminal-shell" className="text-xs">Shell</Label><Help label="Shell">Automático usa o shell de login da sua conta. No macOS, o Jarvis consulta o cadastro do usuário mesmo quando o app não recebe a variável SHELL.</Help></div>
          <Select value={draft.shellChoice} disabled={busy} onValueChange={value => { if (value) patchDraft({ shellChoice: value }); }}>
            <SelectTrigger id="terminal-shell" aria-label="Shell do terminal" className="w-full cursor-pointer font-mono text-xs"><SelectValue>{draft.shellChoice === AUTOMATIC_VALUE ? automaticLabel : draft.shellChoice === CUSTOM_VALUE ? "Personalizado" : `${shellName(draft.shellChoice)} · ${draft.shellChoice}`}</SelectValue></SelectTrigger>
            <SelectContent>
              <SelectItem value={AUTOMATIC_VALUE} className="cursor-pointer text-xs">{automaticLabel}</SelectItem>
              {snapshot.availableTerminalShells.map(shell => <SelectItem key={shell} value={shell} className="cursor-pointer font-mono text-xs">{shellName(shell)} · {shell}</SelectItem>)}
              <SelectItem value={CUSTOM_VALUE} className="cursor-pointer text-xs">Personalizado…</SelectItem>
            </SelectContent>
          </Select>
          {draft.shellChoice === CUSTOM_VALUE && <Input aria-label="Executável personalizado do shell" value={draft.customShell} disabled={busy} maxLength={4096} placeholder="/opt/homebrew/bin/fish" className="font-mono text-xs" onChange={event => patchDraft({ customShell: event.target.value })} />}
        </div>
        <div className="space-y-2">
          <div className="flex items-center gap-1"><Label htmlFor="terminal-arguments" className="text-xs">Argumentos de inicialização</Label><Help label="Argumentos do shell">Use um argumento por linha. Eles são enviados diretamente ao executável, sem interpretação por outro shell. Deixe vazio para usar os argumentos interativos recomendados.</Help></div>
          <Textarea id="terminal-arguments" aria-label="Argumentos de inicialização do shell" value={draft.arguments} disabled={busy} maxLength={8200} spellCheck={false} placeholder={"-l\n-i"} className="min-h-24 resize-y font-mono text-xs" onChange={event => patchDraft({ arguments: event.target.value })} />
          <p className="text-[10px] leading-4 text-muted-foreground">Shell e argumentos afetam apenas terminais novos. Comandos internos dos agentes continuam previsíveis.</p>
        </div>
        {snapshot.terminalError && <p role="alert" className="text-xs text-destructive">{snapshot.terminalError}</p>}
      </Card>

      <Card className="min-w-0 gap-5 p-5">
        <div>
          <h3 className="flex items-center gap-2 text-sm font-medium"><Type className="size-4 text-onedark-purple" />Aparência</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">A fonte é aplicada aos terminais já abertos e aos próximos.</p>
        </div>
        <div className="space-y-2">
          <div className="flex items-center gap-1"><Label htmlFor="terminal-font" className="text-xs">Fonte</Label><Help label="Fonte do terminal">O Jarvis lista as fontes monoespaçadas detectadas no sistema. A opção automática prioriza uma Nerd Font para temas como Powerlevel10k e usa a JetBrains Mono do Jarvis como fallback.</Help></div>
          <Select value={draft.fontChoice} disabled={busy} onValueChange={value => { if (value) patchDraft({ fontChoice: value }); }}>
            <SelectTrigger id="terminal-font" aria-label="Fonte do terminal" className="w-full cursor-pointer text-xs"><SelectValue>{draft.fontChoice === AUTOMATIC_VALUE ? `Automática · ${automaticFont}` : draft.fontChoice === CUSTOM_VALUE ? "Personalizada" : draft.fontChoice}</SelectValue></SelectTrigger>
            <SelectContent>
              <SelectItem value={AUTOMATIC_VALUE} className="cursor-pointer text-xs">Automática · {automaticFont}</SelectItem>
              {snapshot.availableTerminalFonts.map(font => <SelectItem key={font} value={font} className="cursor-pointer text-xs">{font}</SelectItem>)}
              <SelectItem value={CUSTOM_VALUE} className="cursor-pointer text-xs">Personalizada…</SelectItem>
            </SelectContent>
          </Select>
          {draft.fontChoice === CUSTOM_VALUE && <Input aria-label="Família de fonte personalizada" value={draft.customFont} disabled={busy} maxLength={160} placeholder="Nome da fonte instalada" className="text-xs" onChange={event => patchDraft({ customFont: event.target.value })} />}
          {fontWarning && <p role="status" className="text-[10px] leading-4 text-onedark-yellow">{fontWarning}</p>}
        </div>
        <div className="space-y-2">
          <Label htmlFor="terminal-font-size" className="text-xs">Tamanho da fonte</Label>
          <div className="relative max-w-40">
            <Input id="terminal-font-size" aria-label="Tamanho da fonte do terminal" type="number" min={9} max={32} step={1} inputMode="numeric" value={draft.fontSize} disabled={busy} className="pr-12 font-mono text-xs" onChange={event => patchDraft({ fontSize: event.target.value })} />
            <span aria-hidden="true" className="pointer-events-none absolute inset-y-0 right-3 flex items-center font-mono text-[10px] text-muted-foreground">px</span>
          </div>
        </div>
        <div aria-label="Prévia da fonte do terminal" className="overflow-hidden rounded-md border border-border bg-sidebar p-4 text-onedark-green shadow-inner" style={{ fontFamily: terminalFontFamily(resolveTerminalFont(preview.fontFamily, snapshot.availableTerminalFonts)), fontSize: `${preview.fontSize}px` }}>
          <p className="truncate"><span className="text-onedark-purple"></span><span className="bg-onedark-purple px-1 text-background">Jarvis</span><span className="text-onedark-purple"></span> <span className="text-onedark-cyan">~/projeto</span> <span className="text-onedark-yellow"> main</span></p>
          <p className="mt-2 text-foreground"><Sparkles aria-hidden="true" className="mr-2 inline size-3.5 text-onedark-yellow" />cores reais · símbolos do prompt · UTF-8</p>
        </div>
      </Card>
    </div>

    {(error || validationError) && <p role="alert" className="text-xs text-destructive">{error ?? validationError}</p>}
    <div className="flex justify-end">
      <Button type="submit" disabled={busy || !changed} className="cursor-pointer gap-2"><Save className="size-3.5" />{busy ? "Salvando…" : "Salvar preferências"}</Button>
    </div>
  </form></TooltipProvider>;
}
