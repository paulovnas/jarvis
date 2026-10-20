// Development-only native probe: production components, native IPC and real job receipts.
import { createRoot } from "react-dom/client";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Grid2X2 } from "lucide-react";
import { ChatArea } from "@/components/chat/ChatArea";
import { FileWorkspace } from "@/components/files/FileWorkspace";
import { PixelArtCard } from "@/components/chat/PixelArtCard";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Toaster } from "@/components/ui/sonner";
import { useChat } from "@/hooks/use-chat";
import { useWorkflow } from "@/hooks/use-workflow";
import { usePixelArt } from "@/hooks/use-pixel-art";
import { PixelArtResultsContext, usePixelArtResults } from "@/hooks/use-pixel-art-results";
import { accountList, enabledModels, providerDisplayName, type ProviderAccount } from "@/core/provider-accounts";
import { readLibrarySnapshot, type LibrarySnapshot } from "@/core/library";
import { readPixelArtResult } from "@/core/pixel-art";
import "@/index.css";

if (!import.meta.env.DEV) throw new Error("The pixel verification page requires the development server.");
const conversation = new URLSearchParams(location.search).get("conversationId");
if (!conversation || !/^[a-f0-9]{32}$/i.test(conversation)) throw new Error("A native probe conversation is required.");

function clickControl(role: "tab" | "button", name: string) {
  const selector = role === "tab" ? '[role="tab"]' : "button";
  const element = [...document.querySelectorAll<HTMLElement>(selector)]
    .find(node => (node.getAttribute("aria-label") ?? node.textContent ?? "").trim() === name);
  if (!element) throw new Error(`Control unavailable: ${name}`);
  element.click();
  return true;
}

function inspectPreview() {
  const suiteSlot = document.querySelector<HTMLElement>('section[aria-label="Suíte de pixel art"] > div:last-child');
  return {
    conversation: document.querySelector('main[aria-label="Conversa"]')?.getAttribute("aria-description"),
    composer: document.querySelector('[role="textbox"][aria-label="Mensagem"]')?.textContent,
    flow: document.querySelector('[aria-label="Selecionar fluxo"]')?.textContent,
    model: document.querySelector('[aria-label="Selecionar modelo de IA"]')?.textContent,
    tabs: [...document.querySelectorAll('[role="tab"]')].map(node => ({
      title: node.textContent, selected: node.getAttribute("aria-selected"),
    })),
    cards: [...document.querySelectorAll('section[aria-label="Pixel art"]')].map(node => node.textContent),
    // Canvas drawImage uses an animated image's default/first frame by specification.
    // Record its visible bounds so the native screenshot driver can sample rendered frames.
    images: [...document.querySelectorAll<HTMLImageElement>("img")].map(img => ({
      alt: img.alt, mime: img.src.split(";")[0], width: img.naturalWidth, height: img.naturalHeight,
      complete: img.complete, bounds: img.getBoundingClientRect().toJSON(),
    })),
    progress: [...document.querySelectorAll('[role="progressbar"]')].map(node => ({
      value: node.getAttribute("aria-valuenow"), label: node.getAttribute("aria-label"),
    })),
    errors: [...document.querySelectorAll('[role="alert"]')].map(node => node.textContent),
    notices: [...document.querySelectorAll("[data-sonner-toast]")].map(node => node.textContent),
    transcript: document.querySelector('main[aria-label="Conversa"]')?.textContent?.slice(-6000),
    nativeState: window.__jarvisPixelChatState,
    documentState: {
      visibility: document.visibilityState, focused: document.hasFocus(),
      animationFrames: window.__jarvisPixelAnimationFrames,
      width: innerWidth, height: innerHeight, scale: devicePixelRatio,
    },
    suiteSlot: suiteSlot?.getBoundingClientRect().toJSON(),
    approvals: [...document.querySelectorAll("button")]
      .filter(node => node.textContent?.trim() === "Autorizar uma vez")
      .map(node => node.closest('[role="region"][aria-label="Autorização de ferramenta"]')?.textContent),
  };
}

declare global {
  interface Window {
    __jarvisPixelChatState?: unknown;
    __jarvisPixelAnimationFrames?: number;
    __jarvisPixelPreview: {
      inspect: typeof inspectPreview;
      chat: () => boolean;
      suite: () => boolean;
      close: () => boolean;
      logs: () => number;
      type: (text: string) => boolean;
      send: () => boolean;
      approve: () => boolean;
      retry: () => boolean;
      flow: (name: string) => Promise<boolean>;
    };
  }
}
window.__jarvisPixelPreview = {
  inspect: inspectPreview,
  chat: () => {
    clickControl("tab", "Chat");
    setTimeout(() => document.querySelector<HTMLImageElement>('img[src^="data:image/gif"]')?.scrollIntoView({ block: "center", behavior: "instant" }), 100);
    return true;
  },
  suite: () => clickControl("button", "Abrir suíte de pixel art"),
  close: () => clickControl("button", "Fechar suíte de pixel art"),
  logs: () => {
    const buttons = [...document.querySelectorAll<HTMLButtonElement>('button[aria-expanded="false"]')]
      .filter(node => node.textContent?.includes("Logs"));
    buttons.forEach(node => node.click());
    return buttons.length;
  },
  type: text => {
    const editor = document.querySelector<HTMLElement>('[role="textbox"][aria-label="Mensagem"]');
    if (!editor || editor.getAttribute("contenteditable") !== "true") throw new Error("The real chat composer is unavailable.");
    editor.focus();
    document.execCommand("selectAll");
    if (!document.execCommand("insertText", false, text)) throw new Error("The browser did not insert the prompt.");
    return true;
  },
  send: () => clickControl("button", "Enviar mensagem"),
  approve: () => clickControl("button", "Autorizar uma vez"),
  retry: () => clickControl("button", "Tentar novamente"),
  flow: async name => {
    clickControl("button", "Selecionar fluxo");
    for (let attempt = 0; attempt < 20; attempt++) {
      const option = [...document.querySelectorAll<HTMLElement>('[role="option"]')]
        .find(node => node.getAttribute("aria-label") === name);
      if (option) { option.click(); return true; }
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    throw new Error(`Agent unavailable in the actual picker: ${name}`);
  },
};

function NativeChat({ conversationId }: { conversationId: string }) {
  const [context, setContext] = useState<{ library: LibrarySnapshot; accounts: ProviderAccount[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const chat = useChat(context ? conversationId : null);
  const workflow = useWorkflow(context ? conversationId : null);
  useEffect(() => {
    window.__jarvisPixelChatState = {
      history: chat.snapshot?.history,
      turns: chat.snapshot?.turns.length,
      turnResults: chat.snapshot?.turns.map(turn => ({
        id: turn.id, status: turn.status,
        pixelTools: turn.steps.flatMap(step => step.tools).filter(tool => tool.name.startsWith("pixel_art_")).map(tool => {
          const result = readPixelArtResult(tool.output);
          return { id: tool.id, name: tool.name, outputLength: tool.output?.length, valid: result !== null, jobId: result?.jobId, images: result?.images.map(image => image.mime) };
        }),
      })),
      activeTurnId: chat.snapshot?.activeTurnId,
      pendingTurnId: chat.pendingTurn?.id,
    };
    return () => { delete window.__jarvisPixelChatState; };
  }, [chat.snapshot, chat.pendingTurn?.id]);
  useEffect(() => {
    let active = true;
    void invoke<{ library: unknown; accounts: unknown }>("pixel_probe_context", { conversationId }).then(value => {
      const library = readLibrarySnapshot(value.library);
      if (library.selection.conversationId !== conversationId) throw new Error("The private native conversation is not selected.");
      const accounts = accountList(value.accounts);
      if (active) setContext({ library, accounts });
    }).catch(cause => { if (active) setError(String(cause)); });
    return () => { active = false; };
  }, [conversationId]);
  const modelGroups = (context?.accounts ?? [])
    .filter(account => account.enabled && account.modelsAvailable && enabledModels(account).length > 0)
    .map(account => ({
      provider: account.alias,
      displayName: providerDisplayName(account),
      providerKind: account.providerKind,
      modelsStale: account.modelsStale,
      models: enabledModels(account).map(model => ({
        value: `${account.alias}/${model.id}`, label: model.name,
        reasoningLevels: model.reasoningLevels, defaultReasoningLevel: model.defaultReasoningLevel,
        supportsFast: model.supportsFast === true,
      })),
    }));
  return <TooltipProvider><div className="flex h-dvh flex-col bg-background text-foreground">
    {error && <Alert variant="destructive"><AlertTitle>Falha na verificação nativa</AlertTitle><AlertDescription>{error}</AlertDescription></Alert>}
    <ChatArea library={context?.library ?? null} modelGroups={modelGroups} modelsReady={context !== null} chat={chat} workflow={workflow} />
  </div><Toaster /></TooltipProvider>;
}

export function Preview({ conversationId }: { conversationId: string }) {
  const pixel = usePixelArt(conversationId);
  const jobs = usePixelArtResults(conversationId);
  return <TooltipProvider><PixelArtResultsContext value={jobs}><div className="flex h-dvh flex-col bg-background text-foreground">
    <FileWorkspace pixel={pixel} terminalLauncher={<Button variant="ghost" size="icon-sm" aria-label="Abrir suíte de pixel art" onClick={() => void pixel.open()} disabled={pixel.busy}><Grid2X2 className="size-4" /></Button>}>
      <main className="min-h-0 flex-1 overflow-y-auto p-6">
        <div className="mx-auto max-w-3xl space-y-4">
          <p className="text-sm text-muted-foreground">Verificação com execuções reais · componentes do chat do Jarvis</p>
          {Object.values(jobs).map(job => <PixelArtCard key={job.jobId} result={job} />)}
        </div>
      </main>
    </FileWorkspace>
  </div></PixelArtResultsContext><Toaster /></TooltipProvider>;
}
createRoot(document.getElementById("root")!).render(new URLSearchParams(location.search).get("view") === "chat"
  ? <NativeChat key="pixel-receipt-coverage" conversationId={conversation} />
  : <Preview conversationId={conversation} />);

// Observe the actual renderer without changing visibility or replacing image sources.
let animationSample = 0;
const observeAnimationFrame = () => {
  window.__jarvisPixelAnimationFrames = (window.__jarvisPixelAnimationFrames ?? 0) + 1;
  animationSample = requestAnimationFrame(observeAnimationFrame);
};
animationSample = requestAnimationFrame(observeAnimationFrame);
import.meta.hot?.dispose(() => cancelAnimationFrame(animationSample));
