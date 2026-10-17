import { expect, it } from "vitest";
import { accountUsageSchema, aliasSuffix, availableUsageAlertWindows, planLabel, remainingTime, quotaColor, quotaPercent, quotaReserve, expectedQuotaRemaining, usageWindowSchema } from "./provider-usage";
import { shortId } from "./dashboard";

it("preserves optional usage display names and rejects invalid new names without restricting legacy aliases", () => {
  const usage = { alias: "openai-codex-legacy-account-with-a-long-name", fetchedAt: null, email: null, plan: null, windows: [], error: null, resetCredits: null };
  expect(accountUsageSchema.parse(usage)).toEqual(usage);
  expect(accountUsageSchema.parse({ ...usage, displayName: "Conta principal" })).toEqual({ ...usage, displayName: "Conta principal" });
  expect(accountUsageSchema.safeParse({ ...usage, displayName: "x".repeat(19) }).success).toBe(false);
});

it("accepts a reported Hypercredits balance without creating quota windows", () => {
  const usage = { alias: "charm-hyper-pessoal", fetchedAt: 1, email: null, plan: null, windows: [], error: null, resetCredits: null };
  for (const creditBalance of [0, 173, 12.5, null]) {
    expect(accountUsageSchema.parse({ ...usage, creditBalance })).toEqual({ ...usage, creditBalance });
  }
  for (const creditBalance of [-1, Number.NaN, Infinity, "173"]) {
    expect(accountUsageSchema.safeParse({ ...usage, creditBalance }).success).toBe(false);
  }
  expect(aliasSuffix(usage.alias)).toBe("pessoal");
});

it("keeps the complete alias suffix and formats only available quota data", () => {
  expect(aliasSuffix("openai-codex-paulo-trabalho")).toBe("paulo-trabalho");
  expect(aliasSuffix("antigravity-pessoal")).toBe("pessoal");
  expect(aliasSuffix("opencode-go-pessoal-trabalho")).toBe("pessoal-trabalho");
  expect(planLabel("go", "unknown")).toBe("Go");
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

it("uses the Go monthly 30-day pacing horizon without changing its reset or assuming other unknown windows", () => {
  const window = { id: "monthly", label: "Mensal", group: "OpenCode Go", thirdParty: false, durationSeconds: null, remainingPercent: 67, resetsAt: (18 * 24 + 10) * 3600_000 };
  expect(expectedQuotaRemaining(window, 0)).toBeCloseTo(61.3889);
  expect(quotaReserve(window, 0)).toBe(6);
  expect(quotaReserve({ ...window, remainingPercent: 33 }, 0)).toBe(-28);
  expect(quotaReserve({ ...window, remainingPercent: 50, resetsAt: 15 * 86400_000 }, 0)).toBe(0);
  expect(expectedQuotaRemaining({ ...window, durationSeconds: 31 * 86400 }, 0)).toBeCloseTo(59.4086);
  expect(expectedQuotaRemaining({ ...window, group: "Other provider" }, 0)).toBeNull();
  expect(expectedQuotaRemaining({ ...window, id: "unknown" }, 0)).toBeNull();
});

it.each([null, 0, -1, 31 * 86400_000, Number.NaN])("hides Go monthly pacing when its reset is unavailable or outside the 30-day horizon: %s", resetsAt => {
  const window = { id: "monthly", label: "Mensal", group: "OpenCode Go", thirdParty: false, durationSeconds: null, remainingPercent: 67, resetsAt };
  expect(expectedQuotaRemaining(window, 0)).toBeNull();
  expect(quotaReserve(window, 0)).toBeNull();
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

it("keeps partial rolling recovery separate from renewal and pacing", () => {
  const rolling = { id: "five_hour", label: "5h", group: "Charm Hyper", thirdParty: false, durationSeconds: 18_000, remainingPercent: 40, resetsAt: null, nextRecoveryAt: 9_000_000 };
  expect(usageWindowSchema.parse(rolling)).toEqual(rolling);
  expect(usageWindowSchema.parse({ ...rolling, nextRecoveryAt: undefined }).nextRecoveryAt).toBeUndefined();
  expect(expectedQuotaRemaining(rolling, 0)).toBeNull();
  expect(quotaReserve(rolling, 0)).toBeNull();
  expect(expectedQuotaRemaining({ ...rolling, resetsAt: 9_000_000 }, 0)).toBeNull();
  expect(availableUsageAlertWindows([rolling], false)).toEqual(["five_hour"]);
  expect(availableUsageAlertWindows([{ ...rolling, nextRecoveryAt: null }], false)).toEqual([]);
  expect(availableUsageAlertWindows([{ ...rolling, nextRecoveryAt: 0 }], false)).toEqual([]);
  expect(availableUsageAlertWindows([{ ...rolling, nextRecoveryAt: Number.NaN }], false)).toEqual([]);
  expect(availableUsageAlertWindows([{ ...rolling, durationSeconds: 3600 }], false)).toEqual([]);
  expect(availableUsageAlertWindows([{ ...rolling, remainingPercent: null }], false)).toEqual([]);
});
