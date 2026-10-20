import { describe, expect, it } from "vitest";
import { runPixelCockpitVerification, type PixelRuntime } from "./runtime-smoke";

describe("Pixel Cockpit runtime verification control", () => {
  it("does not launch anything before the actual suite is ready", async () => {
    let submissions = 0;
    const runtime: PixelRuntime = {
      start: () => { submissions++; },
      snapshot: () => ({ ready: false }),
    };
    const result = await runPixelCockpitVerification(runtime);
    expect(result.status).toBe("blocked");
    expect(result.error).toContain("not ready");
    expect(submissions).toBe(0);
  });

  it("stops on an actual failed receipt without automatically retrying a command", async () => {
    const requests: string[] = [];
    const runtime: PixelRuntime = {
      start: ({ action }) => { requests.push(action); },
      snapshot: (id) => id ? { id, status: "failed", error: "Runner unavailable" } : { ready: true },
    };
    const result = await runPixelCockpitVerification(runtime, { runRealGeneration: true });
    expect(result.status).toBe("blocked");
    expect(result.error).toContain("Runner unavailable");
    expect(requests).toEqual(["catalog"]);
    expect(result.operations).toHaveLength(1);
  });

  it("requires explicit live-generation opt-in and reports missing real inputs", async () => {
    const requests: string[] = [];
    const runtime: PixelRuntime = {
      start: ({ action }) => { requests.push(action); },
      snapshot: (id) => id ? { id, status: "completed", result: {} } : { ready: true },
    };
    const result = await runPixelCockpitVerification(runtime);
    expect(requests).not.toContain("generate");
    expect(result.status).toBe("blocked");
    expect(result.error).toContain("real animation ID");
    expect(result.unverified.join(" ")).toContain("Real generation was not requested");
  });

  it("continues an independent effect scenario after a terminal image failure", async () => {
    const requests = new Map<string, { action: string; args?: Record<string, unknown> }>();
    const workflows: unknown[] = [];
    const runtime: PixelRuntime = {
      start: ({ id, action, args }) => {
        requests.set(id, { action, args });
        if (action === "generate") workflows.push(args?.workflow);
      },
      snapshot: (id) => {
        if (!id) return { ready: true };
        const request = requests.get(id);
        return request?.action === "generate" && request.args?.workflow === "image-generate"
          ? { id, status: "failed", error: "Image generation failed definitively" }
          : { id, status: "completed", result: { selected: { id: "actual-effect-id", effectAnimation: { frameCount: 4 } } } };
      },
    };
    const result = await runPixelCockpitVerification(runtime, { runRealGeneration: true });
    expect(workflows).toEqual(["image-generate", "effect-animation"]);
    expect(result.assets.effectId).toBe("actual-effect-id");
    expect(result.failures).toHaveLength(1);
    expect(result.status).toBe("blocked");
    expect(result.error).toContain("Image generation failed definitively");
  });

  it.each([true, false])("uses the round-trip receipt status with localized feedback (completed=%s)", async (completed) => {
    const requests = new Map<string, { action: string; args?: Record<string, unknown> }>();
    const runtime: PixelRuntime = {
      start: ({ id, action, args }) => { requests.set(id, { action, args }); },
      snapshot: (id) => {
        if (!id) return { ready: true };
        const request = requests.get(id);
        if (request?.args?.method === "verifyAnimationPackV2RoundTrip") {
          return completed
            ? { id, status: "completed", result: "Pacote v2 exportado e reimportado sem alterações" }
            : { id, status: "failed", error: "Os metadados da linha do tempo foram alterados." };
        }
        if (request?.args?.method === "buildAnimationPackV2EngineExports") {
          return { id, status: "completed", result: Array.from({ length: 5 }, (_, index) => ({ filename: `engine-${index}.json`, data: {} })) };
        }
        return { id, status: "completed", result: { selectedAnimationAction: { frameIds: ["frame-1", "frame-2"] } } };
      },
    };
    const result = await runPixelCockpitVerification(runtime, { animationId: "existing-animation", effectId: "existing-effect" });
    expect(result.status).toBe(completed ? "passed" : "blocked");
    expect([...requests.values()].some(request => request.action === "generate")).toBe(false);
    if (!completed) expect(result.error).toContain("metadados da linha do tempo");
  });

});
