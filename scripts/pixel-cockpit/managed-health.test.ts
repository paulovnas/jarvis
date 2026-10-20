import { expect, it } from "vitest";
import { adoptManagedHealth } from "./managed-health";

it("uses the managed repair boundary and preserves original health and outbox recovery", () => {
  const source = `
async function loadDevSupervisorHealth() { return fetch("http://127.0.0.1:4317/api/dev/health"); }
async function repairCockpit() {
  await postDevSupervisorRepair();
  await runCockpitHealthCheck({ quiet: true });
  await recoverUnimportedOutboxResults();
}
const repairAvailable = Boolean(supervisorHealth);
`;
  const adapted = adoptManagedHealth(source);
  expect(adapted).not.toContain("4317");
  expect(adapted).not.toContain("await postDevSupervisorRepair()");
  expect(adapted).toContain('fetchJsonWithTimeout("/jarvis/repair", { method: "POST" })');
  expect(adapted).toContain("await runCockpitHealthCheck({ quiet: true })");
  expect(adapted).toContain("await recoverUnimportedOutboxResults()");
  expect(adapted).toContain("const repairAvailable = Boolean(apiHealth)");
});
