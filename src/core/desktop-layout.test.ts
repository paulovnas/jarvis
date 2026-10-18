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
  const right = visiblePanels(original, false, 768);
  expect(right["home-sidebar-panel"]).toBe(0);
  expect(right["home-inspector-panel"]).toBeCloseTo(280 / 736 * 100);
  const left = visiblePanels(original, false, 768, "sidebar");
  expect(left["home-sidebar-panel"]).toBeCloseTo(220 / 736 * 100);
  expect(left["home-inspector-panel"]).toBe(0);
  expect(visiblePanels(original, false, 1440)).toEqual({ "home-sidebar-panel": 20, "home-main-panel": 56, "home-inspector-panel": 24 });
  expect(original).toEqual(DEFAULT_DESKTOP_LAYOUT);
});

it("respects explicitly collapsed panels and keeps project navigation on a narrow dashboard", () => {
  expect(visiblePanels({ ...DEFAULT_DESKTOP_LAYOUT, sidebarCollapsed: true, inspectorCollapsed: true }, false, 768)).toEqual({ "home-sidebar-panel": 0, "home-main-panel": 100, "home-inspector-panel": 0 });
  const dashboard = visiblePanels(DEFAULT_DESKTOP_LAYOUT, true, 768);
  expect(dashboard["home-sidebar-panel"]).toBeCloseTo(220 / 736 * 100);
  expect(dashboard).not.toHaveProperty("home-inspector-panel");
});

it("keeps restored sidebars above the pixel minimum without changing saved widths", () => {
  const saved = { ...DEFAULT_DESKTOP_LAYOUT, inspectorCollapsed: true, panels: { "home-sidebar-panel": 14.55, "home-main-panel": 48.216, "home-inspector-panel": 37.234 } };
  for (const width of [600, 768, 1440]) {
    const panels = visiblePanels(saved, false, width);
    expect(panels["home-sidebar-panel"] * (width - 32) / 100).toBeGreaterThanOrEqual(219.99);
    expect(panels["home-inspector-panel"]).toBe(0);
  }
  expect(saved.panels["home-sidebar-panel"]).toBe(14.55);
});

it("uses an expanded width when a legacy zero-sized panel is explicitly opened", () => {
  const saved = { ...DEFAULT_DESKTOP_LAYOUT, panels: { "home-sidebar-panel": 0, "home-main-panel": 100, "home-inspector-panel": 0 } };
  const panels = visiblePanels(saved, false, 768);
  expect(panels["home-sidebar-panel"]).toBe(0);
  expect(panels["home-inspector-panel"]! * 736 / 100).toBeGreaterThanOrEqual(279.99);
});

it("fits open sidebars and the composer within the available viewport", () => {
  const saved = { ...DEFAULT_DESKTOP_LAYOUT, panels: { "home-sidebar-panel": 48, "home-main-panel": 5, "home-inspector-panel": 47 } };
  for (const width of [600, 768, 892, 1440, 3840]) {
    for (const preferred of ["sidebar", "inspector"] as const) {
      const panels = visiblePanels(saved, false, width, preferred);
      const pixels = (id: string) => panels[id as keyof typeof panels]! * (width - 32) / 100;
      expect(pixels("home-main-panel")).toBeGreaterThanOrEqual(Math.min(360, (width - 32) * 0.4) - 0.01);
      if (pixels("home-sidebar-panel") > 0) expect(pixels("home-sidebar-panel")).toBeGreaterThanOrEqual(220 - 0.01);
      if (pixels("home-inspector-panel") > 0) expect(pixels("home-inspector-panel")).toBeGreaterThanOrEqual(280 - 0.01);
      expect(pixels("home-sidebar-panel")).toBeLessThanOrEqual(500.01);
      expect(pixels("home-inspector-panel")).toBeLessThanOrEqual(550.01);
    }
  }
});
