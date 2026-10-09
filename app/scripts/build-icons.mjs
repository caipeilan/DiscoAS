import { spawnSync } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const app = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repository = path.resolve(app, "..");
const source = path.join(repository, "assets/DiscoAS.svg");
const cli = path.join(app, "node_modules/@tauri-apps/cli/tauri.js");
const temporary = await fs.mkdtemp(path.join(os.tmpdir(), "discoas-icons-"));

function generate(output, options = []) {
  const result = spawnSync(process.execPath, [cli, "icon", source, "--output", output, ...options],
    { cwd: app, stdio: "inherit", windowsHide: true });
  if (result.error || result.status !== 0) throw result.error || new Error("Icon generation failed.");
}

try {
  // Render the vector master once for every surface; never resize an old bitmap.
  const desktop = path.join(temporary, "desktop");
  const artwork = path.join(temporary, "artwork");
  generate(desktop);
  generate(artwork, ["--png", "1024"]);
  const files = [
    [path.join(artwork, "1024x1024.png"), path.join(repository, "assets/DiscoAS.png")],
    [path.join(desktop, "icon.ico"), path.join(repository, "assets/Icon.ico")],
    ...["32x32.png", "128x128.png", "128x128@2x.png", "icon.ico", "icon.icns"].map(name =>
      [path.join(desktop, name), path.join(app, "src-tauri/icons", name)]),
    ...["32x32.png", "128x128.png"].map(name =>
      [path.join(desktop, name), path.join(repository, "extensions/browser/icons", name)]),
  ];
  for (const [input, output] of files) await fs.copyFile(input, output);
  console.log("Updated application, installer, tray and browser extension icons.");
} finally {
  // Remove only the directory created by this invocation, inside the resolved temp root.
  if (path.dirname(path.resolve(temporary)) !== path.resolve(os.tmpdir()) ||
      !path.basename(temporary).startsWith("discoas-icons-")) throw new Error("Unexpected icon temporary path.");
  await fs.rm(temporary, { recursive: true, force: true });
}
