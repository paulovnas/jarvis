import { createContext } from "react";

export type DesktopLayout = {
  panels: Record<string, number>;
  inspectorTab: "details" | "activities";
  settingsTab: "general" | "providers" | "skills" | "mcps";
  expandedProjects: Record<string, boolean>;
  activitySections: Record<string, boolean>;
};

export const DEFAULT_DESKTOP_LAYOUT: DesktopLayout = {
  panels: {}, inspectorTab: "activities", settingsTab: "general", expandedProjects: {}, activitySections: {},
};

export type LayoutUpdate = Partial<DesktopLayout> | ((current: DesktopLayout) => Partial<DesktopLayout>);
export const DesktopLayoutContext = createContext<{ layout: DesktopLayout; updateLayout: (update: LayoutUpdate) => void } | null>(null);
