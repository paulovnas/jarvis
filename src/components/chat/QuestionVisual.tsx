import { useState } from "react";
import { Maximize2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Hint } from "@/components/ui/hint";
import type { QuestionPreview } from "@/core/questions";

export function QuestionVisual({ preview, label }: { preview: QuestionPreview; label: string }) {
  if (preview.type === "wireframe") return <svg role="img" aria-label={`Wireframe: ${label}`} viewBox="0 0 400 260" className="block w-full rounded-md border border-border bg-background text-primary">
    {preview.elements.map((region, index) => <g key={index}><rect x={region.x * 4 + 2} y={region.y * 2.6 + 2} width={Math.max(1, region.width * 4 - 4)} height={Math.max(1, region.height * 2.6 - 4)} rx="3" fill="currentColor" fillOpacity="0.07" stroke="currentColor" strokeOpacity="0.35" /><text x={(region.x + region.width / 2) * 4} y={(region.y + region.height / 2) * 2.6} dominantBaseline="middle" textAnchor="middle" fill="currentColor" fontSize="9" fontFamily="monospace" textLength={region.label.length * 5 > region.width * 4 - 8 ? region.width * 4 - 8 : undefined} lengthAdjust="spacingAndGlyphs">{region.label}</text></g>)}
  </svg>;
  if (preview.type === "palette") return <span className="block overflow-hidden rounded-md border border-border bg-background"><span className="flex h-16">{preview.colors.map((color, index) => <Hint key={index} content={color}><span className="flex-1" style={{ backgroundColor: color }} /></Hint>)}</span><span className="block px-3 py-3 text-sm font-medium text-foreground">{preview.sample}</span><span className="block px-3 pb-2 font-mono text-[10px] text-muted-foreground">{preview.colors.join(" · ")}</span></span>;
  return <span role="img" aria-label={`Diagrama: ${label}`} className="block max-h-64 overflow-auto whitespace-pre rounded-md border border-border bg-background p-3 text-left font-mono text-[10px] leading-tight text-foreground">{preview.text}</span>;
}
export function ExpandQuestionVisual({ preview, label }: { preview: QuestionPreview; label: string }) {
  const [open, setOpen] = useState(false);
  return <><Button type="button" variant="ghost" size="sm" className="h-7 cursor-pointer self-end text-[10px] text-muted-foreground" onClick={() => setOpen(true)} aria-label={`Ampliar ${label}`}><Maximize2 className="size-3" />Ampliar</Button><Dialog open={open} onOpenChange={setOpen}><DialogContent className="sm:max-w-3xl" aria-describedby={undefined} onKeyDown={event => { if (event.key === "Escape") event.stopPropagation(); }}><DialogHeader><DialogTitle>{label}</DialogTitle></DialogHeader><QuestionVisual preview={preview} label={label} /></DialogContent></Dialog></>;
}
