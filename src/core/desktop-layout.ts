import { createContext } from "react";

export type DesktopLayout = {
  panels: Record<string, number>;
  inspectorTab: "details" | "activities";
  settingsTab: "general" | "tools" | "providers" | "agents" | "skills" | "mcps";
  expandedProjects: Record<string, boolean>;
  activitySections: Record<string, boolean>;
  sidebarCollapsed: boolean;
  inspectorCollapsed: boolean;
};

export const DEFAULT_DESKTOP_LAYOUT: DesktopLayout = {
  panels: {}, inspectorTab: "activities", settingsTab: "general", expandedProjects: {}, activitySections: {},
  sidebarCollapsed: false, inspectorCollapsed: false,
};

export type LayoutUpdate = Partial<DesktopLayout> | ((current: DesktopLayout) => Partial<DesktopLayout>);
export const DesktopLayoutContext = createContext<{ layout: DesktopLayout; updateLayout: (update: LayoutUpdate) => void } | null>(null);

export function visiblePanels(layout: DesktopLayout, dashboard: boolean) {
  const sidebar = layout.sidebarCollapsed ? 0 : layout.panels["home-sidebar-panel"] ?? 20;
  const inspector = dashboard || layout.inspectorCollapsed ? 0 : layout.panels["home-inspector-panel"] ?? 24;
  return { "home-sidebar-panel": sidebar, "home-main-panel": 100 - sidebar - inspector, ...(!dashboard ? { "home-inspector-panel": inspector } : {}) };
}

export function rememberPanelResize(layout: DesktopLayout, panels: Record<string, number>, dashboard: boolean): Partial<DesktopLayout> {
  const sidebarCollapsed = panels["home-sidebar-panel"] === 0;
  const inspectorCollapsed = dashboard ? layout.inspectorCollapsed : panels["home-inspector-panel"] === 0;
  const sidebar = sidebarCollapsed ? layout.panels["home-sidebar-panel"] ?? 20 : panels["home-sidebar-panel"];
  let inspector = inspectorCollapsed || dashboard ? layout.panels["home-inspector-panel"] ?? 24 : panels["home-inspector-panel"];
  if (dashboard && !sidebarCollapsed) {
    const main = layout.panels["home-main-panel"] ?? 56;
    inspector = (100 - sidebar) * inspector / (main + inspector);
  }
  // Preserve expanded dimensions, even when both visible panels are at zero.
  const scale = Math.min(1, 95 / (sidebar + inspector));
  return { sidebarCollapsed, inspectorCollapsed, panels: { "home-sidebar-panel": sidebar * scale, "home-main-panel": 100 - (sidebar + inspector) * scale, "home-inspector-panel": inspector * scale } };
}
