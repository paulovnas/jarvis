import { expect, it } from "vitest";
import { aliasSuffix, planLabel, remainingTime, quotaColor, quotaPercent } from "./provider-usage";
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
