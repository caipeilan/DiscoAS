import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";
const output = ts.transpileModule(fs.readFileSync(new URL("../src/features/discovery/themePalette.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { discoveryThemePalette, themePalettes } = await import(`data:text/javascript;base64,${Buffer.from(output).toString("base64")}`);
const themeOutput = ts.transpileModule(fs.readFileSync(new URL("../src/theme.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { effectiveNightMode } = await import(`data:text/javascript;base64,${Buffer.from(themeOutput).toString("base64")}`);

const skinOutput = ts.transpileModule(fs.readFileSync(new URL("../src/skins.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { skins, effectiveSkin, skinFrameImage } = await import("data:text/javascript;base64," + Buffer.from(skinOutput).toString("base64"));

test("skin identifiers stay independent of labels and unknown or legacy values select classic", () => {
  assert.deepEqual(skins.map(skin => skin.id), ["classic", "curry-udon"]);
  for (const value of [undefined, null, "", "unknown", "咖喱乌冬面"]) assert.equal(effectiveSkin(value), "classic");
  assert.equal(effectiveSkin("curry-udon"), "curry-udon");
  const image = skinFrameImage("rgb(55, 59, 54)");
  assert.ok(image.startsWith('url("data:image/svg+xml,'));
  assert.ok(decodeURIComponent(image).includes('stroke="rgb(55, 59, 54)"'));
});

test("system appearance follows changes while explicit and legacy choices stay fixed", () => {
  const system = { nightMode: false, followSystemTheme: true };
  assert.equal(effectiveNightMode(system, false), false);
  assert.equal(effectiveNightMode(system, true), true);
  assert.equal(effectiveNightMode({ nightMode: true, followSystemTheme: false }, false), true);
  assert.equal(effectiveNightMode({ nightMode: false }, true), false);
  assert.equal(effectiveNightMode({ nightMode: true }, false), true);
});

test("primary button text meets 4.5:1 contrast in both themes and hover states", () => {
  const css = fs.readFileSync(new URL("../src/styles/tokens.css", import.meta.url), "utf8");
  const luminance = hex => {
    const rgb = hex.match(/[a-f\d]{2}/gi).map(value => parseInt(value, 16) / 255)
      .map(value => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
    return rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
  };
  for (const [declaration, hex] of [...css.matchAll(/--accent(?:-hover)?:\s*(#[a-f\d]{6})/gi)].map(match => [match[0], match[1]])) {
    assert.ok(1.05 / (luminance(hex) + 0.05) >= 4.5, declaration);
  }
});
test("existing default palettes adopt the theme without changing saved preferences", () => {
  const day = Object.freeze({ background: "#FFFFFF", backgroundHover: "#e3f3f6", border: "#76d2fd", fontColor: "#000000" });
  const night = Object.freeze({ background: "#565656", backgroundHover: "#3d75bf", border: "#76d2fd", fontColor: "#ffffff" });
  assert.equal(discoveryThemePalette("card", false, day).border, themePalettes.card.border);
  assert.equal(discoveryThemePalette("card", true, night).background, themePalettes.card_night.background);
  assert.equal(day.border, "#76d2fd");
  assert.equal(night.background, "#565656");
});
test("a partially customized palette is preserved as a whole", () => {
  const saved = { background: "#FFFFFF", backgroundHover: "#e3f3f6", border: "#d08a21", fontColor: "#000000" };
  assert.strictEqual(discoveryThemePalette("card", false, saved), saved);
});
test("old day and night close controls adopt distinct opaque theme colors", () => {
  const day = { background: "#fecbc1", backgroundHover: "#fd8b76", border: "#fc6044", fontColor: "#000000" };
  const night = { background: "#400601", backgroundHover: "#bd0316", border: "#fc6044", fontColor: "#ffffff" };
  assert.equal(discoveryThemePalette("cancel", false, day).fontColor, themePalettes.cancel.fontColor);
  assert.equal(discoveryThemePalette("cancel", true, night).fontColor, themePalettes.cancel_night.fontColor);
  assert.strictEqual(discoveryThemePalette("cancel", true, day), day);
});

test("unchanged appearance snapshots skip root writes while real appearance and system-theme changes apply", async () => {
  const saved = {document:globalThis.document,window:globalThis.window,CSS:globalThis.CSS,getComputedStyle:globalThis.getComputedStyle};
  let writes=0, systemDark=false;
  let frameReads=0;
  const values=new Map();
  const root={dataset:new Proxy({}, {set(target,key,value){writes++;target[key]=value;return true;}}),
    style:{setProperty(key,value){writes++;values.set(key,value);},removeProperty(key){writes++;values.delete(key);},set fontFamily(value){writes++;}}};
  globalThis.document={documentElement:root};
  globalThis.window={matchMedia:()=>({matches:systemDark})};
  globalThis.CSS={supports:()=>true};
  globalThis.getComputedStyle=()=>{frameReads++;return {getPropertyValue:()=>systemDark ? "#a1a38b" : "#575b50"};};
  globalThis.__appearanceTest={discoveryThemePalette,effectiveNightMode,effectiveSkin,skinFrameImage};
  try {
    const output=ts.transpileModule(fs.readFileSync(new URL("../src/appearance.ts",import.meta.url),"utf8"),{
      compilerOptions:{target:ts.ScriptTarget.ES2020,module:ts.ModuleKind.ESNext},
    }).outputText.replace(/import\s+\{([^}]+)\}\s+from\s+"[^"]+";/g,(_,names)=>`const {${names}}=globalThis.__appearanceTest;`);
    const {applyAppearance}=await import(`data:text/javascript;base64,${Buffer.from(output).toString("base64")}`);
    const gui={fontFamily:"",fontSize:14,mainWindowScale:1,discoveryCardScale:1,cancelButtonScale:1,replacementButtonScale:1,discoveryBarScale:1,
      followSystemTheme:true,nightMode:false,card:{},cancelButton:{},cardNightMode:{},cancelButtonNightMode:{}};
    applyAppearance(gui);const firstWrites=writes;
    applyAppearance(structuredClone(gui));assert.equal(writes,firstWrites);
    applyAppearance({...gui,fontSize:16});assert.equal(values.get("--font-scale"),String(16/14));
    systemDark=true;applyAppearance(gui);assert.equal(root.dataset.theme,"dark");
    systemDark=false;
    const colors=Object.freeze({background:"#123456",backgroundHover:"#234567",border:"#345678",fontColor:"#ffffff"});
    const classic={...gui,skin:"classic",card:colors,cardNightMode:colors};
    applyAppearance(classic);assert.equal(values.get("--card-background"),"#123456");
    const curry={...classic,skin:"curry-udon"};
    applyAppearance(curry);
    assert.equal(root.dataset.skin,"curry-udon");
    assert.equal(values.has("--card-background"),false);
    const border=values.get("--skin-frame"),reads=frameReads;
    assert.ok(border.includes("data:image/svg+xml"));
    applyAppearance({...curry,fontSize:18});
    assert.equal(frameReads,reads,"font changes do not rebuild the border images");
    systemDark=true;applyAppearance(curry);
    assert.notEqual(values.get("--skin-frame"),border);
    applyAppearance(classic);
    assert.equal(root.dataset.skin,"classic");
    assert.equal(values.get("--card-background"),"#123456");
    assert.equal(values.has("--skin-frame"),false);
    assert.equal(colors.background,"#123456");
  } finally {Object.assign(globalThis,saved);delete globalThis.__appearanceTest;}
});
