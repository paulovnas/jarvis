import type { WorkflowAppearance } from "@/core/workflow-appearance";
import { workflowAppearance } from "./workflow-appearance";

export function WorkflowIdentityIcon({ appearance, fallback, className }: { appearance?: WorkflowAppearance | null; fallback?: WorkflowAppearance; className?: string }) {
  const { Icon, color } = workflowAppearance(appearance, fallback);
  return <Icon aria-hidden="true" className={className} style={{ color }} />;
}
