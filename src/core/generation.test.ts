import { describe, expect, it } from "vitest";
import { combineGeneration, generationRate, generationSchema } from "./generation";

describe("generation throughput", () => {
  it("weights the average by model time instead of averaging individual speeds", () => {
    const generation = combineGeneration([
      { outputTokens: 100, durationMs: 1_000, estimated: false },
      { outputTokens: 100, durationMs: 9_000, estimated: false },
    ]);
    expect(generationRate(generation)).toBe(20);
  });

  it("keeps a mixed live average marked as approximate until all counts are confirmed", () => {
    const completed = { outputTokens: 100, durationMs: 2_000, estimated: false };
    const partial = { outputTokens: 50, durationMs: 3_000, estimated: true };
    expect(combineGeneration([completed, partial])).toEqual({ outputTokens: 150, durationMs: 5_000, estimated: true });
    expect(combineGeneration([completed, { ...partial, estimated: false }])?.estimated).toBe(false);
  });

  it("includes a silent model call in model time without counting unmeasured steps", () => {
    const generation = combineGeneration([
      undefined, null,
      { outputTokens: 100, durationMs: 2_000, estimated: false },
      { outputTokens: 0, durationMs: 3_000, estimated: true },
    ]);
    expect(generationRate(generation)).toBe(20);
  });

  it("does not invent throughput for older histories, zero output or a first buffered instant", () => {
    expect(combineGeneration([undefined, null])).toBeNull();
    expect(generationRate(undefined)).toBeNull();
    expect(generationRate({ outputTokens: 0, durationMs: 1_000, estimated: false })).toBeNull();
    expect(generationRate({ outputTokens: 50, durationMs: 20, estimated: true })).toBeNull();
    expect(generationRate({ outputTokens: 50, durationMs: Number.NaN, estimated: true })).toBeNull();
  });

  it("rejects invalid wire metrics", () => {
    expect(generationSchema.safeParse({ outputTokens: -1, durationMs: 1_000, estimated: false }).success).toBe(false);
    expect(generationSchema.safeParse({ outputTokens: 10, durationMs: -1, estimated: false }).success).toBe(false);
  });
});
