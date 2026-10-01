import { useEffect, useState, type RefObject } from "react";

/** Read the native CSS timeline so steps stay aligned with island travel, including pauses. */
export function useCompanionStroll(pet: RefObject<HTMLElement | null>, active: boolean) {
  const [pose, setPose] = useState({ walking: false, facing: 1 });
  const [query] = useState(() => window.matchMedia("(prefers-reduced-motion: reduce)"));
  const [reducedMotion, setReducedMotion] = useState(() => query.matches);
  useEffect(() => {
    const changed = () => setReducedMotion(query.matches);
    query.addEventListener("change", changed);
    return () => query.removeEventListener("change", changed);
  }, [query]);
  useEffect(() => {
    if (!active || reducedMotion) return;
    const sample = () => {
      const animation = pet.current?.parentElement?.getAnimations?.().find(animation =>
        "animationName" in animation && animation.animationName === "companion-stroll");
      const duration = animation?.effect?.getTiming().duration;
      const phase = animation && typeof duration === "number" && duration > 0 && typeof animation.currentTime === "number"
        ? (animation.currentTime % duration) / duration : 0;
      const walking = animation?.playState === "running" && (phase > .2 && phase < .32 || phase > .44 && phase < .52 || phase > .61 && phase < .74);
      const facing = phase < .44 ? -1 : 1;
      setPose(previous => previous.walking === walking && previous.facing === facing ? previous : { walking, facing });
    };
    const frame = window.requestAnimationFrame(sample);
    const timer = window.setInterval(sample, 180);
    return () => { window.cancelAnimationFrame(frame); window.clearInterval(timer); };
  }, [pet, active, reducedMotion]);
  return active && !reducedMotion ? pose : { walking: false, facing: 1 };
}
