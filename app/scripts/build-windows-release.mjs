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

function run(command, arguments_, cwd = app) {
  const result = spawnSync(command, arguments_, { cwd, stdio: "inherit", windowsHide: true });
  if (result.error || result.status !== 0) throw result.error || new Error(`${command} exited with ${result.status}`);
}
async function build() {
  const command = [cli, "build", "--bundles", "nsis"];
  run(process.execPath, command);
  const bytes = await fs.readFile(executable);
  const pe = bytes.readUInt32LE(0x3c);
  if (bytes.subarray(pe, pe + 4).toString("binary") !== "PE\0\0" || bytes.readUInt16LE(pe + 4) !== 0x8664)
    throw new Error("Desktop executable is not AMD64.");
  if (!fullOnly) {
    const name = `DiscoAS_${version}_x64-setup.exe`;
    await fs.copyFile(bundle, path.join(output, name));
    console.log(`Prepared ${name}`);
  }
}
await build();
if (!normalOnly) {
  const source = path.resolve(process.env.DISCOAS_WEBVIEW2_INSTALLER || path.join(repository, ".local-release/MicrosoftEdgeWebView2RuntimeInstallerX64.exe"));
  // Reuse the local installer and this build's generated NSIS script without a download lookup.
  run("powershell.exe", ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File",
    path.join(app, "scripts/prepare-webview2.ps1"), "-SourcePath", source]);
  const generated = path.join(app, "src-tauri/target/release/nsis/x64");
  const offline = path.join(app, "src-tauri/target/release/nsis/x64-full");
  await fs.mkdir(offline, { recursive: true });
  for (const filename of await fs.readdir(generated)) {
    if (filename.endsWith(".nsh")) await fs.copyFile(path.join(generated, filename), path.join(offline, filename));
  }
  const script = (await fs.readFile(path.join(generated, "installer.nsi"), "utf8"))
    .replace('!define INSTALLWEBVIEW2MODE "downloadBootstrapper"', '!define INSTALLWEBVIEW2MODE "offlineInstaller"')
    .replace('!define WEBVIEW2INSTALLERPATH ""', `!define WEBVIEW2INSTALLERPATH "${source}"`);
  const installerScript = path.join(offline, "installer.nsi");
  await fs.writeFile(installerScript, script, "utf8");
  const compiler = path.join(process.env.LOCALAPPDATA, "tauri/NSIS/makensis.exe");
  run(compiler, ["/V2", "/INPUTCHARSET", "UTF8", installerScript], offline);
  const name = `DiscoAS_${version}_x64-setup-full.exe`;
  await fs.copyFile(path.join(offline, "nsis-output.exe"), path.join(output, name));
  console.log(`Prepared ${name}`);
}
