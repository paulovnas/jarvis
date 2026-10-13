import type { ComputerApp, ComputerSnapshot } from "@/core/computer-use";

export const computerApp: ComputerApp = {
  appId: "safari", name: "Safari", processId: 77, launchId: "launch-1",
  identity: { bundleId: "com.apple.Safari", bundlePath: "/Applications/Safari.app", executablePath: "/Applications/Safari.app/Contents/MacOS/Safari" },
  windows: [{ id: 8, title: "Documentação", bounds: { x: 0, y: 0, width: 900, height: 600 } }],
};

export function computerSnapshot(overrides: Partial<ComputerSnapshot> = {}): ComputerSnapshot {
  return {
    mode: "manual",
    capabilities: { platform: "macos", supported: true, accessibility: true, screenCapture: true, inputPosting: true, captureSupported: true, sessionStatus: "interactive", reason: null },
    grants: [], targets: [], activity: null, stopped: false,
    ...overrides,
  };
}

export const computerTarget = {
  id: "safari-window", appId: "safari", appName: "Safari", processId: 77,
  windowId: 8, windowTitle: "Documentação", access: "control" as const, generation: 1, revision: 3,
};
