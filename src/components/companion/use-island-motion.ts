import { useLayoutEffect, useRef, type RefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CompanionGeometry } from "@/core/companion";

interface Frame {
  x: number; y: number; width: number; height: number; radius: number;
  petX: number; petY: number; petSize: number;
}
const keys = ["x", "y", "width", "height", "radius", "petX", "petY", "petSize"] as const;
const zeroVelocity: Frame = { x: 0, y: 0, width: 0, height: 0, radius: 0, petX: 0, petY: 0, petSize: 0 };

/** One interruptible spring drives the shell and the same living character. */
export function useIslandMotion(island: RefObject<HTMLDivElement | null>, pet: RefObject<HTMLDivElement | null>, geometry: CompanionGeometry, open: boolean, detail: boolean, visible: boolean) {
  const state = useRef({
    frame: { x: 0, y: 0, width: 288, height: 32, radius: 18, petX: 20, petY: 2, petSize: 28 } as Frame,
    velocity: { ...zeroVelocity }, compactX: 0, compactY: 0,
  });
  useLayoutEffect(() => {
    if (!island.current || !pet.current) return;
    const shell = island.current;
    const character = pet.current;
    const spring = state.current;
    const notchInset = geometry.notchWidth > 0 ? geometry.headerHeight : 0;
    spring.frame.x += geometry.compactX - spring.compactX;
    spring.frame.y += geometry.compactY - spring.compactY;
    spring.compactX = geometry.compactX; spring.compactY = geometry.compactY;
    const target: Frame = open ? {
      x: geometry.surfaceX, y: geometry.surfaceY, width: geometry.surfaceWidth, height: geometry.surfaceHeight,
      radius: 28, petX: detail ? 16 : 24, petY: notchInset + (detail ? 9 : 55), petSize: detail ? 24 : 80,
    } : { x: geometry.compactX, y: geometry.compactY, width: geometry.compactWidth, height: geometry.compactHeight, radius: 18, petX: 20, petY: Math.max(0, (geometry.compactHeight - 28) / 2), petSize: 28 };
    let frameId = 0;
    let previous = performance.now();
    let lastReported = -Infinity;
    const paint = () => {
      const current = spring.frame;
      Object.assign(shell.style, { left: `${current.x}px`, top: `${current.y}px`, width: `${current.width}px`, height: `${current.height}px`, borderRadius: notchInset > 0 ? `0 0 ${current.radius}px ${current.radius}px` : `${current.radius}px` });
      Object.assign(character.style, { left: `${current.petX}px`, top: `${current.petY}px`, width: `${current.petSize}px`, height: `${current.petSize}px` });
    };
    const report = () => {
      const x = Math.max(0, Math.min(spring.frame.x, geometry.width - 1));
      const y = Math.max(0, Math.min(spring.frame.y, geometry.height - 1));
      void invoke("companion_set_hit_rect", { x, y, width: Math.max(1, Math.min(spring.frame.width, geometry.width - x)), height: Math.max(1, Math.min(spring.frame.height, geometry.height - y)) }).catch(() => {});
    };
    const tick = (now: number) => {
      const dt = Math.max(0, Math.min((now - previous) / 1000, 1 / 30)); previous = now;
      let moving = false;
      const steps = Math.max(1, Math.ceil(dt * 120));
      const step = dt / steps;
      for (const key of keys) {
        const stiffness = open ? 380 : 620;
        const damping = open && key.startsWith("pet") ? 29 : open ? 37 : 50;
        for (let index = 0; index < steps; index++) {
          spring.velocity[key] += (stiffness * (target[key] - spring.frame[key]) - damping * spring.velocity[key]) * step;
          spring.frame[key] += spring.velocity[key] * step;
        }
        if (Math.abs(target[key] - spring.frame[key]) > .1 || Math.abs(spring.velocity[key]) > .5) moving = true;
      }
      if (!moving) { spring.frame = { ...target }; spring.velocity = { ...zeroVelocity }; }
      paint();
      if (!moving || now - lastReported >= 32) { report(); lastReported = now; }
      if (moving) frameId = window.requestAnimationFrame(tick);
    };
    if (!visible || window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      spring.frame = { ...target }; spring.velocity = { ...zeroVelocity }; paint(); report();
    } else { paint(); report(); frameId = window.requestAnimationFrame(tick); }
    return () => window.cancelAnimationFrame(frameId);
  }, [island, pet, geometry, open, detail, visible]);
}
