import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

const nativeRoot = new URL("../src-tauri/", import.meta.url);
const read = (file) => fs.readFileSync(new URL(file, nativeRoot), "utf8").replace(/\r\n/g, "\n");
const template = read("installer/installer.nsi"), hooks = read("installer/hooks.nsh");
const config = JSON.parse(read("tauri.conf.json"));
const block = (source, start, end) => {
  source = source.replace(/\r\n/g, "\n");
  const first = source.indexOf(start), last = source.indexOf(end, first + start.length);
  assert.ok(first >= 0 && last > first, `Missing block ${start}`);
  return source.slice(first + start.length, last);
};
const runtime = block(template, "Section WebView2\n", "SectionEnd");

test("minimum runtime and frontend targets agree, with three installer languages", () => {
  assert.equal(config.bundle.windows.minimumWebview2Version, "109.0.0.0");
  assert.match(read("../vite.config.ts"), /target:\s*"edge109"/);
  assert.equal(config.bundle.windows.nsis.template, "installer/installer.nsi");
  assert.deepEqual(config.bundle.windows.nsis.languages, ["SimpChinese", "TradChinese", "English"]);
});

test("AMD64 and Windows 10 build checks precede maintenance and runtime actions", () => {
  const init = block(template, "Function .onInit\n", "FunctionEnd");
  const guard = init.indexOf("!insertmacro DISCOAS_VALIDATE_SYSTEM");
  assert.ok(guard > init.indexOf("!insertmacro MUI_LANGDLL_DISPLAY"));
  assert.ok(guard < init.indexOf("!insertmacro SetContext"));
  assert.match(hooks, /\$\{IfNot\} \$\{IsNativeAMD64\}/);
  assert.match(hooks, /\$\{IfNot\} \$\{AtLeastWin10\}/);
  assert.match(hooks, /\$\{OrIfNot\} \$\{AtLeastBuild\} 17763/);
  assert.match(hooks, /\$\{OrIf\} \$\{IsServerOS\}/);
  assert.match(hooks, /MessageBox[^\n]*\/SD IDOK/);
  assert.match(hooks, /SetErrorLevel 1\s+Abort/);
});

test("missing and old runtimes share the selected mode, including app upgrades", () => {
  const readVersion = block(template, "Function DiscoASReadWebView2Version\n", "FunctionEnd");
  assert.match(readVersion, /SetRegView 32[\s\S]*ReadRegStr \$4 HKLM[\s\S]*ReadRegStr \$5 HKCU[\s\S]*SetRegView lastused/);
  assert.match(readVersion, /\$\{VersionCompare\} "\$5" "\$4" \$R0/);
  assert.doesNotMatch(template, /Function DiscoASIsWebView2Version/);
  assert.equal((runtime.match(/Call DiscoASReadWebView2Version/g) ?? []).length, 2);
  const compare = runtime.indexOf('${VersionCompare} "${MINIMUMWEBVIEW2VERSION}" "$4"');
  assert.ok(compare >= 0 && compare < runtime.indexOf('!if "${INSTALLWEBVIEW2MODE}"'));
  assert.doesNotMatch(runtime, /\$UpdateMode|IDIGNORE|needsadmin/);
  for (const mode of ["downloadBootstrapper", "offlineInstaller"]) {
    const branch = block(runtime, `!if "\${INSTALLWEBVIEW2MODE}" == "${mode}"`, "!endif");
    assert.match(branch, /Goto install_webview2/);
    if (mode === "offlineInstaller") {
      assert.match(branch, /File [^\n]*\$\{WEBVIEW2INSTALLERPATH\}/);
      assert.doesNotMatch(branch, /NSISdl::download|ExecWait/);
    }
  }
});

test("runtime failures stop installation, and installed version is checked again", () => {
  const install = block(runtime, "install_webview2:\n", "webview2_done:");
  assert.match(install, /ClearErrors\s+ExecWait/);
  assert.match(install, /\$\{If\} \$\{Errors\}[\s\S]*SetErrorLevel 1\s+Abort/);
  assert.match(install, /\$\{ElseIf\} \$1 <> 0[\s\S]*SetErrorLevel 1\s+Abort/);
  const verify = install.indexOf("Call DiscoASReadWebView2Version");
  assert.ok(verify > install.indexOf("ExecWait") && verify < install.indexOf('DetailPrint "$(webview2InstallSuccess)"'));
  assert.match(install.slice(verify), /\$\{If\} \$4 == ""[\s\S]*SetErrorLevel 1\s+Abort/);
  assert.match(install.slice(verify), /\$\{VersionCompare\}[^\n]+[\s\S]*\$\{If\} \$R0 = 1[\s\S]*SetErrorLevel 1\s+Abort/);
});

// Check actual generated scripts after both installer builds.
test("ordinary/full generated scripts use their intended runtime sources", {
  skip: !process.env.DISCOAS_NSIS_SCRIPTS,
}, () => {
  const scripts = JSON.parse(process.env.DISCOAS_NSIS_SCRIPTS);
  assert.equal(scripts.length, 2);
  scripts.forEach((script, index) => {
    assert.ok(path.isAbsolute(script));
    const source = fs.readFileSync(script, "utf8");
    assert.match(source, /!define ARCH "x64"/);
    assert.match(source, /!define MINIMUMWEBVIEW2VERSION "109\.0\.0\.0"/);
    assert.ok(source.includes(`!define INSTALLWEBVIEW2MODE "${index ? "offlineInstaller" : "downloadBootstrapper"}"`));
    const runtime = block(source, "Section WebView2\n", "SectionEnd");
    const mode = index ? "offlineInstaller" : "downloadBootstrapper";
    const branch = block(runtime, `!if "\${INSTALLWEBVIEW2MODE}" == "${mode}"`, "!endif");
    if (index) {
      assert.doesNotMatch(branch, /(?:NSISdl|inetc)::|Exec(?:Wait)?/);
      const installer = source.match(/!define WEBVIEW2INSTALLERPATH "([^"]+)"/);
      assert.ok(installer && fs.existsSync(installer[1]));
      assert.match(branch, /File [^\n]*\$\{WEBVIEW2INSTALLERPATH\}/);
    } else assert.match(branch, /NSISdl::download/);
    assert.match(branch, /Goto install_webview2/);
    assert.doesNotMatch(runtime, /Exec(?:Wait)?[^\n]*(?:EdgeUpdate|https?:)/i);
  });
});
