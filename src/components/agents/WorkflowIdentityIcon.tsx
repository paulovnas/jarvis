import type { IdentityAppearance } from "@/core/workflow-appearance";
import { workflowAppearance } from "./workflow-appearance";

export function WorkflowIdentityIcon({ appearance, fallback, className }: { appearance?: IdentityAppearance | null; fallback?: IdentityAppearance; className?: string }) {
  const { Icon, color } = workflowAppearance(appearance, fallback);
  return <Icon aria-hidden="true" className={className} style={{ color }} />;
}
