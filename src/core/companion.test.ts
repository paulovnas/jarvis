import { expect, it } from "vitest";
import { companionGeometrySchema } from "./companion";

it("keeps the compact island origin in the larger native canvas for smooth closing", () => {
  const geometry = companionGeometrySchema.parse({
    expanded: true, bubble: false, robotSide: "left", robotVertical: "top",
    width: 640, height: 160, compactX: 176, compactY: 0, compactWidth: 288, compactHeight: 32,
    surfaceX: 0, surfaceY: 0, surfaceWidth: 640, surfaceHeight: 160,
  });
  expect(geometry).toMatchObject({ compactX: 176, compactY: 0, compactWidth: 288, compactHeight: 32, surfaceWidth: 640, surfaceHeight: 160 });
  expect(companionGeometrySchema.parse({ expanded: false, bubble: false, robotSide: "left", robotVertical: "top", width: 288, height: 32 }))
    .toMatchObject({ compactX: 0, compactY: 0, compactWidth: 288, compactHeight: 32, surfaceWidth: 288, surfaceHeight: 32 });
});

it("preserves the camera exclusion area and platform drag policy", () => {
  const geometry = companionGeometrySchema.parse({
    expanded: false, bubble: false, robotSide: "left", robotVertical: "top", width: 314, height: 38,
    notchWidth: 210, notchHeight: 38, headerHeight: 38, dragAxis: "none",
  });
  expect(geometry).toMatchObject({ notchWidth: 210, notchHeight: 38, headerHeight: 38, dragAxis: "none" });
  expect(companionGeometrySchema.safeParse({ ...geometry, dragAxis: "vertical" }).success).toBe(false);
});
