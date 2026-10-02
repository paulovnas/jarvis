import { useCallback, useEffect, useState } from "react";
import type { RobotProps } from "./Robot";

/** Rest belongs to one uninterrupted idle period, not the snapshot polling clock. */
export function useRobotRest(blocked: boolean, activityKey: string) {
  const [rest, setRest] = useState({ blocked, activityKey, interaction: 0, gesture: "none" as NonNullable<RobotProps["gesture"]> });
  // Reset during rendering so a new activity never displays a stale sleeping pose.
  if (rest.blocked !== blocked || rest.activityKey !== activityKey) {
    setRest({ ...rest, blocked, activityKey, gesture: "none" });
  }
  const wake = useCallback(() => {
    setRest(previous => ({ ...previous, interaction: previous.interaction + 1, gesture: "none" }));
  }, []);
  const interaction = rest.interaction;
  useEffect(() => {
    if (blocked) return;
    let alive = true;
    const schedule = (delay: number, gesture: NonNullable<RobotProps["gesture"]>) => window.setTimeout(() => {
      if (alive) setRest(previous => ({ ...previous, gesture }));
    }, delay);
    const timers = [schedule(25_000, "curious"), schedule(27_200, "none"), schedule(65_000, "stretch"), schedule(67_200, "none"), schedule(120_000, "sleep")];
    return () => { alive = false; timers.forEach(timer => window.clearTimeout(timer)); };
  }, [blocked, activityKey, interaction]);
  return { gesture: rest.blocked === blocked && rest.activityKey === activityKey ? rest.gesture : "none", wake };
}
