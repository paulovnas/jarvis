import { lazy, Suspense } from "react";
import type { CompanionStatus } from "@/core/companion";
import { RobotFallback } from "./RobotFallback";

const RiveRobot = lazy(() => import("./RiveRobot"));

export interface RobotProps {
  status: CompanionStatus;
  visible?: boolean;
  hovered?: boolean;
  dragging?: boolean;
  expanded?: boolean;
  walking?: boolean;
  gesture?: "none" | "wink" | "surprised" | "sleepy" | "poke" | "dizzy" | "sleep" | "stretch" | "curious";
  lookX?: number;
  lookY?: number;
}

export function Robot(props: RobotProps) {
  return <Suspense fallback={<RobotFallback status={props.status} gesture={props.gesture} renderer="loading" />}>
    <RiveRobot {...props} />
  </Suspense>;
}
