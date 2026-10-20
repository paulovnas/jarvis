/** Capture state inside the write queue so a stale response cannot overwrite a terminal receipt. */
export function createSnapshotWriter(capture: () => unknown, save: (serialized: string) => Promise<void>) {
  let tail = Promise.resolve();
  let persistedVersion = "";
  return () => {
    const write = tail.then(async () => {
      const snapshot = capture();
      if (snapshot === undefined) return;
      const serialized = JSON.stringify(snapshot);
      if (serialized === persistedVersion) return;
      await save(serialized);
      persistedVersion = serialized;
    });
    // The caller receives its own failure; later saves remain independent of it.
    tail = write.catch(() => undefined);
    return write;
  };
}
