import { createContext } from "react";

export type TerminalPanelLayout = { open: boolean; size: number; activeTerminalId: string | null };
export const DEFAULT_TERMINAL_PANEL: TerminalPanelLayout = { open: false, size: 40, activeTerminalId: null };
export type FileTabsLayout = { paths: string[]; activePath: string | null };
export const DEFAULT_FILE_TABS: FileTabsLayout = { paths: [], activePath: null };

export type DesktopLayout = {
  panels: Record<string, number>;
  inspectorTab: "details" | "activities" | "github" | "explorer" | "designer";
  settingsTab: "general" | "terminal" | "browser" | "voice" | "tools" | "providers" | "agents" | "skills" | "mcps" | "hooks" | "plugins" | "workspaces";
  itemOrder: Record<string, string[]>;
  expandedProjects: Record<string, boolean>;
  activitySections: Record<string, boolean>;
  sidebarCollapsed: boolean;
  inspectorCollapsed: boolean;
  terminalPanels: Record<string, TerminalPanelLayout>;
  fileTabs: Record<string, FileTabsLayout>;
  lastSeenReleaseVersion: string | null;
};

export const DEFAULT_DESKTOP_LAYOUT: DesktopLayout = {
  panels: {}, inspectorTab: "activities", settingsTab: "general", expandedProjects: {}, activitySections: {},
  sidebarCollapsed: false, inspectorCollapsed: false, terminalPanels: {}, fileTabs: {}, itemOrder: {}, lastSeenReleaseVersion: null,
};

export type LayoutUpdate = Partial<DesktopLayout> | ((current: DesktopLayout) => Partial<DesktopLayout>);
export const DesktopLayoutContext = createContext<{ layout: DesktopLayout; updateLayout: (update: LayoutUpdate) => void } | null>(null);

export function rememberTerminalPanel(layout: DesktopLayout, key: string, update: Partial<TerminalPanelLayout>): Partial<DesktopLayout> {
  return { terminalPanels: { ...layout.terminalPanels, [key]: { ...DEFAULT_TERMINAL_PANEL, ...layout.terminalPanels[key], ...update } } };
}

// Reserve 220px + 360px + 280px for content, plus 32px of gutters.
export const COMPACT_LAYOUT_WIDTH = 892;

export function visiblePanels(layout: DesktopLayout, dashboard: boolean, windowWidth = Infinity, preferred: "sidebar" | "inspector" = "inspector") {
  let sidebar = layout.sidebarCollapsed ? 0 : layout.panels["home-sidebar-panel"] || 20;
  let inspector = dashboard || layout.inspectorCollapsed ? 0 : layout.panels["home-inspector-panel"] || 24;
  if (windowWidth < COMPACT_LAYOUT_WIDTH && sidebar > 0 && inspector > 0) {
    if (preferred === "sidebar") inspector = 0;
    else sidebar = 0;
  }
  if (Number.isFinite(windowWidth)) {
    const available = Math.max(1, windowWidth - 32);
    const sidebarMin = sidebar > 0 ? Math.min(60, 220 / available * 100) : 0;
    const inspectorMin = inspector > 0 ? Math.min(60, 280 / available * 100) : 0;
    // Clamp for display only: restored percentages can otherwise snap an open
    // collapsible panel to zero pixels while its controls still report it open.
    if (sidebar > 0) sidebar = Math.min(500 / available * 100, Math.max(sidebarMin, sidebar));
    if (inspector > 0) inspector = Math.min(550 / available * 100, Math.max(inspectorMin, inspector));
    const budget = 100 - Math.min(40, 360 / available * 100);
    const extra = sidebar - sidebarMin + inspector - inspectorMin;
    if (sidebar + inspector > budget && extra > 0) {
      const scale = Math.max(0, budget - sidebarMin - inspectorMin) / extra;
      sidebar = sidebarMin + (sidebar - sidebarMin) * scale;
      inspector = inspectorMin + (inspector - inspectorMin) * scale;
    }
  }
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
