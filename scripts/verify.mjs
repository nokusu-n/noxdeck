// Every automated entry point uses a separate instance and startup value.
import { spawn } from "node:child_process";
import { startOverfast } from "../e2e/overfast.mjs";
import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const temporaryRoot = join(root, "e2e", ".tmp");
mkdirSync(temporaryRoot, { recursive: true });
const temporary = mkdtempSync(join(temporaryRoot, "verify-"));
const overfast = await startOverfast();
const env = { ...process.env, TEMP: temporary, TMP: temporary, TMPDIR: temporary,
  CARGO_TARGET_DIR: join(root, "target"),
  NOXDECK_INSTANCE: `verify-${randomUUID()}`,
  NOXDECK_AUTOSTART_NAME: `NoxDeck-Test-${randomUUID()}`,
  NOXDECK_LOCALE: "en-US", NOXDECK_OVERFAST_URL: overfast.url };
const run = (command, args) => new Promise((resolve, reject) => {
  console.log(`VERIFY ${command} ${args.join(" ")}`);
  const child = spawn(command, args, { cwd: root, env, stdio: "inherit", windowsHide: true });
  child.on("error", reject);
  child.on("exit", code => code === 0 ? resolve() : reject(new Error(`Verification exited ${code}`)));
});
const tauri = args => run(process.execPath, [join(root, "node_modules", "@tauri-apps", "cli", "tauri.js"), "build", ...args]);
const mode = process.argv[2] ?? "all";
if (!["all", "core", "program", "installer"].includes(mode)) throw new Error("Use all, core, program or installer");
try {
  if (mode === "all" || mode === "core") await run("cargo", ["test", "--workspace"]);
  if (mode === "all") await run(process.execPath, ["e2e/run.mjs"]);
  if (mode === "program") {
    env.CARGO_TARGET_DIR = join(root, "target", "e2e");
    await tauri(["--debug", "--no-bundle"]); await run("cargo", ["build", "-p", "test-launcher"]);
  }
  if (mode === "all" || mode === "program") await run("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "scripts/program-tests.ps1", ...(process.env.PROGRAM_ONLY ? ["-Only", process.env.PROGRAM_ONLY] : [])]);
  if (mode === "installer") await tauri([]);
  console.log(`VERIFY ${mode}: passed`);
} finally {
  await overfast.stop();
  if (!resolve(temporary).startsWith(temporaryRoot + sep)) throw new Error("Temporary cleanup escaped the workspace");
  rmSync(temporary, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
}
