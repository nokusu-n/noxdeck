// `npm run e2e`: build the app (debug, interface embedded) and the stand-in launcher,
// then run every *.spec.mjs through the running app, one file at a time.
// E2E_SKIP_BUILD=1 reuses the last build; E2E_ONLY=name runs the matching spec files.

import { execSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readdirSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const temp = join(root, "e2e", ".tmp");
mkdirSync(temp, { recursive: true });
const buildTemporary = mkdtempSync(join(temp, "build-"));
// The app for the tests is built into its own target directory: a plain `cargo test`
// would otherwise overwrite it with a development build that loads the dev server.
const env = {
  ...process.env,
  PATH: `${process.env.PATH};${join(homedir(), ".cargo", "bin")}`,
  CARGO_TARGET_DIR: join(root, "target", "e2e"),
  NOXDECK_INSTANCE: `e2e-build-${process.pid}`,
  NOXDECK_AUTOSTART_NAME: `NoxDeck-Test-e2e-build-${process.pid}`,
  NOXDECK_LOCALE: "en-US",
  NOXDECK_OVERFAST_URL: "http://127.0.0.1:9",
  TEMP: buildTemporary,
  TMP: buildTemporary,
};
const run = (cmd) => execSync(cmd, { cwd: root, env, stdio: "inherit" });

try {
if (!process.env.E2E_SKIP_BUILD) {
  run("npx tauri build --debug --no-bundle");
  run("cargo build -p fake-launcher");
}

const only = process.env.E2E_ONLY;
const specs = readdirSync(join(root, "e2e"))
  .filter((f) => f.endsWith(".spec.mjs") && (!only || f.includes(only)))
  .map((f) => join("e2e", f));

const r = spawnSync(process.execPath, ["--test", "--test-concurrency=1", "--test-timeout=180000", ...(process.env.E2E_TEST_NAME ? [`--test-name-pattern=${process.env.E2E_TEST_NAME}`] : []), ...specs], {
  cwd: root,
  env,
  stdio: "inherit",
});
process.exitCode = r.status ?? 1;
} finally {
  if (!resolve(buildTemporary).startsWith(temp + "\\")) throw new Error("Build cleanup escaped the workspace");
  rmSync(buildTemporary, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
}
