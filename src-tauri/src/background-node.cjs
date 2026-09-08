// Jarvis's headless Node integrations run runtime probes before their own
// Windows-aware executors are initialized. Keep those synchronous probes hidden.
if (process.platform === "win32") {
  const childProcess = require("node:child_process");
  const hidden = (options) => ({ ...options, windowsHide: true });
  const execSync = childProcess.execSync;
  childProcess.execSync = function (command, options) {
    return execSync.call(this, command, hidden(options));
  };
  for (const name of ["execFileSync", "spawnSync"]) {
    const original = childProcess[name];
    childProcess[name] = function (file, args, options) {
      return Array.isArray(args)
        ? original.call(this, file, args, hidden(options))
        : original.call(this, file, hidden(args ?? options));
    };
  }
  require("node:module").syncBuiltinESMExports();
}
