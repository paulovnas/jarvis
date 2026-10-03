// Firefox exposes promise-based WebExtension APIs under `browser`; Chromium
// exposes the common APIs under `chrome`. Browser-specific APIs stay in adapters.
export function extensionApi(): typeof chrome {
  const runtime = globalThis as unknown as { browser?: typeof chrome; chrome?: typeof chrome };
  const api = runtime.browser ?? runtime.chrome;
  if (!api) throw new Error("A API da extensão não está disponível.");
  return api;
}

export function isFirefox(): boolean {
  return extensionApi().runtime.getURL?.("").startsWith("moz-extension://") ?? false;
}

export async function restrictStorageAccess(): Promise<void> {
  const { storage } = extensionApi();
  // Firefox session storage is already restricted to trusted extension contexts
  // and does not implement Chrome's setAccessLevel method.
  if (storage.local.setAccessLevel) await storage.local.setAccessLevel({ accessLevel: "TRUSTED_CONTEXTS" });
  if (storage.session.setAccessLevel) await storage.session.setAccessLevel({ accessLevel: "TRUSTED_CONTEXTS" });
}
