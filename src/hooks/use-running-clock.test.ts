import { expect, it } from "vitest";
import { executionDuration } from "./use-running-clock";

it("excludes human, dependency and offline waits while preserving work before a retry", () => {
  const hour = 3_600_000;
  expect(executionDuration(0, 20_000, true, 9 * hour, null)).toBe(20_000);
  expect(executionDuration(0, 0, true, 9 * hour, null)).toBe(0);
  expect(executionDuration(0, 20_000, true, 9 * hour + 5_000, 9 * hour)).toBe(25_000);
  expect(executionDuration(0, 25_000, false, 10 * hour, null)).toBe(25_000);
});

it("keeps legacy history readable and clamps clock skew", () => {
  expect(executionDuration(1_000, 100, true, 2_000)).toBe(1_000);
  expect(executionDuration(0, 100, true, 1_000, 2_000)).toBe(100);
});
