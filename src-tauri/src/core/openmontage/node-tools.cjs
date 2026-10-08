// Resolve managed renderers independently of the user's composition directory.
const { join, resolve } = require("node:path");
const { spawnSync } = require("node:child_process");
const { existsSync } = require("node:fs");
const generation = resolve(__dirname, "..");
const args = process.argv.slice(2);
while (args[0] === "--yes" || args[0] === "-y") args.shift();
const renderer = args.shift();
const entries = {
  hyperframes: join(generation, "node_modules/hyperframes/bin/hyperframes.mjs"),
  remotion: join(generation, "OpenMontage/remotion-composer/node_modules/@remotion/cli/remotion-cli.js"),
};
if (!Object.prototype.hasOwnProperty.call(entries, renderer)) {
  process.stderr.write("OpenMontage only invokes its installed rendering CLIs. Configure additional dependencies explicitly.\n");
  process.exit(1);
}
if (renderer === "remotion" && (args[0] === "ensure-browser" || (args[0] === "browser" && args[1] === "ensure"))) {
  const browser = process.env.REMOTION_BROWSER_EXECUTABLE;
  if (!browser || !existsSync(browser)) {
    process.stderr.write("The managed browser is unavailable. Repair OpenMontage in Core settings.\n");
    process.exit(1);
  }
  process.stdout.write("The managed browser is ready.\n");
  process.exit(0);
}
if (renderer === "remotion" && ["render", "still", "benchmark", "compositions", "bundle"].includes(args[0])) {
  // Managed dependencies remain read-only inside the native production sandbox.
  args.push("--bundle-cache=false");
}
if (renderer === "remotion" && ["render", "still", "benchmark"].includes(args[0]) && process.env.REMOTION_BROWSER_EXECUTABLE
    && !args.some((arg) => arg.startsWith("--browser-executable"))) {
  args.push(`--browser-executable=${process.env.REMOTION_BROWSER_EXECUTABLE}`);
}
const result = spawnSync(process.execPath, [entries[renderer], ...args], { stdio: "inherit" });
if (result.error) {
  process.stderr.write("The managed rendering runtime could not be started.\n");
  process.exit(1);
}
process.exit(result.status ?? 1);
