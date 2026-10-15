import { useEffect, useState } from "react";
import { Sparkles } from "lucide-react";

import { REFINEMENT_MESSAGES } from "./prompt-refinement-messages";

function randomMessage(previous?: number) {
  const index = Math.floor(Math.random() * (REFINEMENT_MESSAGES.length - (previous === undefined ? 0 : 1)));
  return previous !== undefined && index >= previous ? index + 1 : index;
}

export function PromptRefinementStatus() {
  const [message, setMessage] = useState(() => randomMessage());
  useEffect(() => {
    const timer = window.setInterval(() => setMessage(previous => randomMessage(previous)), 4_000);
    return () => window.clearInterval(timer);
  }, []);
  return <div role="status" aria-label="Refinando prompt" className="flex h-full flex-col items-center justify-center gap-1 px-5 py-1 text-center">
    <Sparkles aria-hidden="true" className="size-4 shrink-0 text-onedark-purple" />
    <p className="text-sm font-medium">Refinando o texto…</p>
    <p className="text-xs leading-4 text-muted-foreground">{REFINEMENT_MESSAGES[message]}</p>
    <span aria-hidden="true" className="flex gap-1">{[0, 1, 2].map(index => <span key={index} className="size-1 rounded-full bg-onedark-purple/75 motion-safe:animate-pulse" style={{ animationDelay: `${index * 150}ms` }} />)}</span>
  </div>;
}
