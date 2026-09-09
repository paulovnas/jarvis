import { expect, it } from "vitest";
import { aliasSuffix, availableUsageAlertWindows, planLabel, remainingTime, quotaColor, quotaPercent, quotaReserve, expectedQuotaRemaining } from "./provider-usage";
import { shortId } from "./dashboard";

it("keeps the complete alias suffix and formats only available quota data", () => {
  expect(aliasSuffix("openai-codex-paulo-trabalho")).toBe("paulo-trabalho");
  expect(aliasSuffix("antigravity-pessoal")).toBe("pessoal");
  expect(planLabel("self_serve_business_prolite", "enterprise")).toBe("Business");
  expect(planLabel("g1-ultra-tier", "unknown")).toBe("Google AI Ultra");
  expect(planLabel(null, "unknown")).toBe("Não informado");
  expect(remainingTime(72 * 60_000, 0)).toBe("1h 12m");
  expect(remainingTime((6 * 24 + 5) * 3600_000, 0)).toBe("6d 5h");
  expect(remainingTime(null, 0)).toBeNull();
  expect(remainingTime(1, 2)).toBe("agora");
  expect(quotaPercent(null)).toBe("—");
  expect(quotaPercent(0)).toBe("0%");
  expect(quotaColor(0)).toBe("#e06c75");
  expect(quotaColor(20)).toBe("#e5c07b");
});
it("shows readable project references while preserving native beads IDs and child suffixes", () => {
  expect(shortId("jarvis-0ht.3", "Jarvis")).toBe("jarvis-0ht.3");
  expect(shortId(`j${"a".repeat(32)}-1234567890abcdef.3`, "Lindoya Verão")).toBe("lindoya-verao-123456….3");
});

it("calcula reserva e deficit relativos ao tempo restante sem inventar janelas", () => {
  const window = { id: "weekly", label: "7d", group: "Codex", thirdParty: false, durationSeconds: 7 * 86400, remainingPercent: 78, resetsAt: 3.5 * 86400_000 };
  expect(quotaReserve(window, 0)).toBe(28);
  expect(expectedQuotaRemaining(window, 0)).toBe(50);
  expect(expectedQuotaRemaining({ ...window, resetsAt: 7 * 86400_000 }, 0)).toBe(100);
  expect(expectedQuotaRemaining({ ...window, resetsAt: 1.75 * 86400_000 }, 0)).toBe(25);
  expect(expectedQuotaRemaining(window, window.resetsAt)).toBeNull();
  expect(expectedQuotaRemaining({ ...window, durationSeconds: 0 }, 0)).toBeNull();
  expect(expectedQuotaRemaining({ ...window, resetsAt: Number.NaN }, 0)).toBeNull();
  expect(quotaReserve({ ...window, remainingPercent: 76, resetsAt: 6.5 * 86400_000 }, 0)).toBe(-17);
  expect(quotaReserve({ ...window, remainingPercent: null }, 0)).toBeNull();
  expect(quotaReserve({ ...window, durationSeconds: null }, 0)).toBeNull();
  expect(quotaReserve({ ...window, resetsAt: null }, 0)).toBeNull();
  expect(quotaReserve(window, window.resetsAt)).toBeNull();
  expect(quotaReserve({ ...window, resetsAt: 8 * 86400_000 }, 0)).toBeNull();
});

it("derives only alert windows actually returned by the provider", () => {
  const base = { id: "limit", label: "Cota", group: "Gemini", thirdParty: false, remainingPercent: 50, resetsAt: 1 };
  expect(availableUsageAlertWindows([
    { ...base, id: "5h", durationSeconds: 18_000 },
    { ...base, id: "7d", durationSeconds: 604_800 },
    { ...base, id: "daily", durationSeconds: 86_400 },
    { ...base, id: "third", durationSeconds: 18_000, thirdParty: true },
  ], false)).toEqual(["five_hour", "weekly"]);
  expect(availableUsageAlertWindows([{ ...base, id: "third", durationSeconds: 18_000, thirdParty: true }], false)).toEqual([]);
  expect(availableUsageAlertWindows([{ ...base, id: "third", durationSeconds: 18_000, thirdParty: true }], true)).toEqual(["five_hour"]);
});
