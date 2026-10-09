import { spawnSync } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const app = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repository = path.resolve(app, "..");
const output = path.join(repository, "release");
const args = new Set(process.argv.slice(2));
const normalOnly = args.has("--normal-only");
const fullOnly = args.has("--full-only");
if (normalOnly && fullOnly) throw new Error("Choose only one installer variant.");
if (process.platform !== "win32" || process.arch !== "x64") throw new Error("Build on Windows x64.");
const manifest = JSON.parse(await fs.readFile(path.join(app, "package.json"), "utf8"));
const version = manifest.version;
if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error("Stable release version is required.");
await fs.mkdir(output, { recursive: true });
const cli = path.join(app, "node_modules/@tauri-apps/cli/tauri.js");
const bundle = path.join(app, "src-tauri/target/release/bundle/nsis", `DiscoAS_${version}_x64-setup.exe`);
const executable = path.join(app, "src-tauri/target/release/discoas.exe");

function run(command, arguments_) {
  const result = spawnSync(command, arguments_, { cwd: app, stdio: "inherit", windowsHide: true });
  if (result.error || result.status !== 0) throw result.error || new Error(`${command} exited with ${result.status}`);
}
async function build(full) {
  const command = [cli, "build", "--bundles", "nsis"];
  if (full) command.push("--config", "src-tauri/tauri.full.conf.json");
  run(process.execPath, command);
  const bytes = await fs.readFile(executable);
  const pe = bytes.readUInt32LE(0x3c);
  if (bytes.subarray(pe, pe + 4).toString("binary") !== "PE\0\0" || bytes.readUInt16LE(pe + 4) !== 0x8664)
    throw new Error("Desktop executable is not AMD64.");
  const name = `DiscoAS_${version}_x64-setup${full ? "-full" : ""}.exe`;
  await fs.copyFile(bundle, path.join(output, name));
  console.log(`Prepared ${name}`);
}
if (!fullOnly) await build(false);
if (!normalOnly) {
  const source = process.env.DISCOAS_WEBVIEW2_INSTALLER || path.join(repository, ".local-release/MicrosoftEdgeWebView2RuntimeInstallerX64.exe");
  // A complete, official file must be prepared before the full bundle can be built.
  run("powershell.exe", ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File",
    path.join(app, "scripts/prepare-webview2.ps1"), "-SourcePath", source]);
  await build(true);
}
