import { createContext } from "react";

export type TerminalPanelLayout = { open: boolean; size: number; activeTerminalId: string | null };
export const DEFAULT_TERMINAL_PANEL: TerminalPanelLayout = { open: false, size: 40, activeTerminalId: null };
export type FileTabsLayout = { paths: string[]; activePath: string | null };
export const DEFAULT_FILE_TABS: FileTabsLayout = { paths: [], activePath: null };

export type DesktopLayout = {
  panels: Record<string, number>;
  inspectorTab: "details" | "activities" | "explorer";
  settingsTab: "general" | "tools" | "providers" | "agents" | "skills" | "mcps";
  expandedProjects: Record<string, boolean>;
  activitySections: Record<string, boolean>;
  sidebarCollapsed: boolean;
  inspectorCollapsed: boolean;
  terminalPanels: Record<string, TerminalPanelLayout>;
  fileTabs: Record<string, FileTabsLayout>;
};

export const DEFAULT_DESKTOP_LAYOUT: DesktopLayout = {
  panels: {}, inspectorTab: "activities", settingsTab: "general", expandedProjects: {}, activitySections: {},
  sidebarCollapsed: false, inspectorCollapsed: false, terminalPanels: {}, fileTabs: {},
};

export type LayoutUpdate = Partial<DesktopLayout> | ((current: DesktopLayout) => Partial<DesktopLayout>);
export const DesktopLayoutContext = createContext<{ layout: DesktopLayout; updateLayout: (update: LayoutUpdate) => void } | null>(null);

export function rememberTerminalPanel(layout: DesktopLayout, conversationId: string, update: Partial<TerminalPanelLayout>): Partial<DesktopLayout> {
  return { terminalPanels: { ...layout.terminalPanels, [conversationId]: { ...DEFAULT_TERMINAL_PANEL, ...layout.terminalPanels[conversationId], ...update } } };
}

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
