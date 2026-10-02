import { useEffect, useLayoutEffect, useMemo, useState } from "react";
import { Alignment, EventType, Fit, Layout, RuntimeLoader, StateMachineInputType, useRive, type StateMachineInput } from "@rive-app/react-canvas";
import wasmUrl from "@rive-app/canvas/rive.wasm?url";
import wasmFallbackUrl from "@rive-app/canvas/rive_fallback.wasm?url";
import robotUrl from "@/assets/companion/jarvito.riv?url";
import type { CompanionStatus } from "@/core/companion";
import type { RobotProps } from "./Robot";
import { RobotFallback } from "./RobotFallback";

// Both binaries and the character are bundled; the floating pet works offline.
RuntimeLoader.setWasmUrl(wasmUrl);
RuntimeLoader.setWasmFallbackUrl(wasmFallbackUrl);

const machine = "Jarvito";
const layout = new Layout({ fit: Fit.Contain, alignment: Alignment.Center });
const statuses: Record<CompanionStatus, number> = { idle: 0, running: 1, waiting: 2, reconnecting: 3, completed: 4, failed: 5 };
const gestures = { none: 0, wink: 1, surprised: 2, sleepy: 3, poke: 4, dizzy: 5, sleep: 6, stretch: 7, curious: 8, listen: 9, speak: 10 };
const inputTypes = {
  status: StateMachineInputType.Number,
  gesture: StateMachineInputType.Number,
  hovered: StateMachineInputType.Boolean,
  dragging: StateMachineInputType.Boolean,
  expanded: StateMachineInputType.Boolean,
  walking: StateMachineInputType.Boolean,
  lookX: StateMachineInputType.Number,
  lookY: StateMachineInputType.Number,
  reducedMotion: StateMachineInputType.Boolean,
  voiceLevel: StateMachineInputType.Number,
};
const gaze = (value: number) => Number.isFinite(value) ? Math.max(-1, Math.min(1, value)) : 0;
function applyInputs(inputs: Record<keyof typeof inputTypes, StateMachineInput>, values: Record<keyof typeof inputTypes, number | boolean>) {
  // Rive's public input handles are mutable runtime controls, not React state.
  for (const name of Object.keys(values) as (keyof typeof inputTypes)[]) inputs[name].value = values[name];
}

export default function RiveRobot({ status, visible = true, hovered = false, dragging = false, expanded = false, walking = false, gesture = "none", lookX = 0, lookY = 0, voiceLevel = 0 }: RobotProps) {
  const [failed, setFailed] = useState(false);
  const [pageHidden, setPageHidden] = useState(() => document.hidden);
  const [motionQuery] = useState(() => window.matchMedia("(prefers-reduced-motion: reduce)"));
  const [reducedMotion, setReducedMotion] = useState(motionQuery.matches);
  const { rive, RiveComponent } = useRive({
    src: robotUrl, artboard: machine, stateMachines: machine, layout,
    autoplay: false, enableRiveAssetCDN: false, shouldDisableRiveListeners: true,
    onLoadError: () => setFailed(true),
  }, { shouldUseIntersectionObserver: false });
  const inputs = useMemo(() => {
    if (!rive) return null;
    const available = new Map((rive.stateMachineInputs(machine) ?? []).map(input => [input.name, input]));
    if (Object.entries(inputTypes).some(([name, type]) => available.get(name)?.type !== type)) return null;
    // The complete public input contract was checked above before binding it.
    return Object.fromEntries(available) as Record<keyof typeof inputTypes, StateMachineInput>;
  }, [rive]);
  const ready = Boolean(rive && inputs && !failed);

  useEffect(() => {
    const visibility = () => setPageHidden(document.hidden);
    const motion = () => setReducedMotion(motionQuery.matches);
    document.addEventListener("visibilitychange", visibility);
    motionQuery.addEventListener("change", motion);
    return () => {
      document.removeEventListener("visibilitychange", visibility);
      motionQuery.removeEventListener("change", motion);
    };
  }, [motionQuery]);

  useLayoutEffect(() => {
    if (!rive) return;
    if (!inputs || failed || !visible || pageHidden) {
      rive.pause(); rive.stopRendering();
      return;
    }
    applyInputs(inputs, {
      status: statuses[status], gesture: gestures[gesture], hovered, dragging, expanded,
      walking: walking && gesture !== "sleep" && !hovered && !dragging && !reducedMotion,
      lookX: gesture === "sleep" ? 0 : gaze(lookX), lookY: gesture === "sleep" ? 0 : gaze(lookY), reducedMotion,
      voiceLevel: reducedMotion ? 0 : Math.max(0, gaze(voiceLevel)),
    });
    if (reducedMotion) {
      // Apply one current pose, then stop. Keep the same character and instance.
      const freeze = () => { rive.off(EventType.Advance, freeze); rive.pause(); rive.stopRendering(); };
      rive.on(EventType.Advance, freeze);
      rive.play(machine);
      return () => { rive.off(EventType.Advance, freeze); };
    }
    if (!rive.isPlaying) rive.play(machine);
    else rive.startRendering();
  }, [rive, inputs, failed, visible, pageHidden, status, hovered, dragging, expanded, walking, gesture, lookX, lookY, reducedMotion, voiceLevel]);

  return <div aria-hidden="true" className="companion-robot" data-state={status} data-gesture={gesture} data-renderer={ready ? "rive" : failed || (rive && !inputs) ? "fallback" : "loading"} data-motion={reducedMotion ? "reduced" : "full"} style={{ position: "relative", pointerEvents: "none" }}>
    <div style={{ position: "absolute", inset: 0, visibility: ready ? "visible" : "hidden" }}><RiveComponent aria-hidden="true" tabIndex={-1} /></div>
    {!ready && <RobotFallback status={status} gesture={gesture} voiceLevel={voiceLevel} renderer={failed || rive ? "fallback" : "loading"} />}
  </div>;
}
