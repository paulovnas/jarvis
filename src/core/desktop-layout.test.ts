import { expect, it } from "vitest";
import { DEFAULT_DESKTOP_LAYOUT, rememberPanelResize, visiblePanels } from "./desktop-layout";

it("restores expanded widths after collapsing both panels and switching through Dashboard", () => {
  const original = { ...DEFAULT_DESKTOP_LAYOUT, panels: { "home-sidebar-panel": 23, "home-main-panel": 51, "home-inspector-panel": 26 } };
  const collapsed = { ...original, ...rememberPanelResize(original, { "home-sidebar-panel": 0, "home-main-panel": 100, "home-inspector-panel": 0 }, false) };
  expect(collapsed.panels).toEqual(original.panels);
  expect(visiblePanels(collapsed, false)).toEqual({ "home-sidebar-panel": 0, "home-main-panel": 100, "home-inspector-panel": 0 });
  expect(visiblePanels(collapsed, true)).toEqual({ "home-sidebar-panel": 0, "home-main-panel": 100 });
  expect(visiblePanels({ ...collapsed, sidebarCollapsed: false, inspectorCollapsed: false }, false)).toEqual(original.panels);
});

it("keeps the hidden panel's dimensions while resizing the other sidebar", () => {
  const original = { ...DEFAULT_DESKTOP_LAYOUT, sidebarCollapsed: true };
  expect(rememberPanelResize(original, { "home-sidebar-panel": 0, "home-main-panel": 70, "home-inspector-panel": 30 }, false)).toEqual({ sidebarCollapsed: true, inspectorCollapsed: false, panels: { "home-sidebar-panel": 20, "home-main-panel": 50, "home-inspector-panel": 30 } });
});

it("keeps one requested side panel in a portrait window and restores both at desktop width", () => {
  const original = structuredClone(DEFAULT_DESKTOP_LAYOUT);
  expect(visiblePanels(original, false, 768)).toEqual({ "home-sidebar-panel": 0, "home-main-panel": 76, "home-inspector-panel": 24 });
  expect(visiblePanels(original, false, 768, "sidebar")).toEqual({ "home-sidebar-panel": 20, "home-main-panel": 80, "home-inspector-panel": 0 });
  expect(visiblePanels(original, false, 1440)).toEqual({ "home-sidebar-panel": 20, "home-main-panel": 56, "home-inspector-panel": 24 });
  expect(original).toEqual(DEFAULT_DESKTOP_LAYOUT);
});

it("respects explicitly collapsed panels and keeps project navigation on a narrow dashboard", () => {
  expect(visiblePanels({ ...DEFAULT_DESKTOP_LAYOUT, sidebarCollapsed: true, inspectorCollapsed: true }, false, 768)).toEqual({ "home-sidebar-panel": 0, "home-main-panel": 100, "home-inspector-panel": 0 });
  expect(visiblePanels(DEFAULT_DESKTOP_LAYOUT, true, 768)).toEqual({ "home-sidebar-panel": 20, "home-main-panel": 80 });
});
