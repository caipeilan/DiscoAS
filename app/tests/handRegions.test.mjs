import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";
const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/regionUpdates.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { LatestRegionUpdate, HandBoundsCache } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };

test("region bursts serialize native updates and flush the latest drag position", async () => {
  const gate = deferred(), measurements = []; let position = 1, inFlight = 0, maximum = 0;
  const queue = new LatestRegionUpdate(async () => {
    maximum = Math.max(maximum, ++inFlight); measurements.push(position);
    if (measurements.length === 1) await gate.promise;
    inFlight--;
  });
  const first = queue.run(); await Promise.resolve();
  position = 2; queue.run(); position = 3; const last = queue.run();
  gate.resolve(); await Promise.all([first, last]);
  assert.deepEqual(measurements, [1, 3]); assert.equal(maximum, 1);
});

test("a new request at completion is not lost, and a failed native call can be retried", async () => {
  let calls = 0, fail = true;
  const queue = new LatestRegionUpdate(async () => { calls++; if (fail) throw Error("native failure"); });
  await assert.rejects(queue.run(), /native failure/); fail = false;
  await queue.run();
  await queue.run().then(() => queue.run());
  assert.equal(calls, 4);
});

test("static cards reuse bounds while hover, drag and final animation frames remain measurable", () => {
  let reads = 0, moving = false, position = 1;
  const card = { getBoundingClientRect() { reads++; return { left: position, top: 0, right: position+10, bottom: 10, width: 10, height: 10 }; },
    getAnimations: () => moving ? [{ playState: "running", pending: false }] : [] };
  const cache = new HandBoundsCache();
  for (let i = 0; i < 30; i++) assert.equal(cache.read(card).left, 1);
  assert.equal(reads, 1);
  moving = true; position = 2; assert.equal(cache.read(card).left, 2);
  position = 3; assert.equal(cache.read(card).left, 3);
  moving = false; position = 4; assert.equal(cache.read(card).left, 4);
  assert.equal(cache.read(card).left, 4);
  position = 5; assert.equal(cache.read(card, true).left, 5);
  position = 6; assert.equal(cache.read(card).left, 6);
  cache.reset(); position = 7; assert.equal(cache.read(card).left, 7);
  assert.equal(reads, 7);
});

test("an immediate fast drag waits for the native drawing surface before painting and committing release", async () => {
  const compile = file => ts.transpileModule(fs.readFileSync(new URL(file,import.meta.url),"utf8"),{compilerOptions:{target:ts.ScriptTarget.ES2020,module:ts.ModuleKind.ESNext}}).outputText;
  const layout = await import(`data:text/javascript;base64,${Buffer.from(compile("../src/features/hand/handLayout.ts")).toString("base64")}`);
  const gate = deferred(), paint = [], operations = [], dragStates = [], values = new Map();
  const pose = {x:200,y:400,width:156,height:220,angle:7,scale:1.06};
  const element = { style:{setProperty:(name,value)=>{values.set(name,value);paint.push([name,value]);},removeProperty:name=>values.delete(name)},
    focus(){},getAnimations:()=>[],setPointerCapture(){},hasPointerCapture:()=>false };
  const card = {id:"one"}, snapshot = {generation:1,expanded:true,preview:false,cards:[card],settings:{side:"bottom",scale:1},workArea:{left:0,top:0,width:1000,height:700},gui:{font_size:14}};
  const context = {snapshot,current:{current:snapshot},mounted:{current:true},dockMoving:false,stage:{current:null},busyRef:{current:""},selectedRef:{current:null},hoverRef:{current:null},inputRef:{current:"pointer"},pointer:{current:null},toolbarBounds:{current:null},posesRef:{current:[pose]},restPosesRef:{current:[pose]},dragRef:{current:null},dragFrame:{current:0},dragDrawing:{current:null},
    setDrag:value=>dragStates.push(value),setInputMode(){},setHover(){},setSelected(){},setSnapshot(){},setError:error=>assert.fail(error),toCanvas:(x,y)=>({x,y}),scheduleRegions:{current(){}},commitRegions:{current:()=>gate.promise},operation:async(name,card,pose)=>operations.push({name,id:card.id,pose})};
  const imports = {react:"react","../../services/desktop":"bridge","../../i18n":"i18n","../settings/discoveryKeybindings":"keys","./handLayout":"layout","./handDrawingBounds":"bounds","./handMotion":"motion","./handControllerTypes":"types"};
  globalThis.__handInputTest = {react:{useRef:value=>({current:value}),useEffect(){}},bridge:{desktop:true,call:async()=>{},onDesktopEvent:async()=>()=>{}},i18n:{errorText:String},keys:{keybindingFromEvent:()=>""},layout,bounds:{handPoseBounds:p=>({left:p.x-100,top:p.y-100,width:200,height:200})},motion:{releaseHandMotion(){}},types:{readPose:()=>({...pose,x:pose.x+parseFloat(values.get("--card-x")||0),y:pose.y+parseFloat(values.get("--card-y")||0)})}};
  const saved = {requestAnimationFrame:globalThis.requestAnimationFrame,cancelAnimationFrame:globalThis.cancelAnimationFrame};
  globalThis.requestAnimationFrame=()=>{assert.fail("movement was queued before its drawing surface was ready");}; globalThis.cancelAnimationFrame=()=>{};
  try {
    const hook = compile("../src/features/hand/useHandInput.ts").replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g,(_,names,path)=>`const {${names}} = globalThis.__handInputTest.${imports[path]};`);
    const {useHandInput} = await import(`data:text/javascript;base64,${Buffer.from(hook).toString("base64")}`);
    const input = useHandInput(context);
    input.onCardPointerDown(card,{button:0,pointerId:8,clientX:200,clientY:400,currentTarget:element,target:{closest:()=>null},preventDefault(){}});
    input.onCardPointerMove(card,{pointerId:8,clientX:480,clientY:240});
    assert.equal(paint.length,0);
    assert.equal(dragStates.length,1);
    const release = input.endDrag({pointerId:8});
    input.lostPointerCapture("one");
    assert.equal(operations.length,0); assert.ok(context.dragRef.current);
    gate.resolve(); await release;
    assert.ok(paint.some(([name,value])=>name==="--card-x"&&value==="280px"));
    assert.equal(operations.length,1); assert.equal(operations[0].name,"play_hand_card");
    assert.equal(operations[0].pose.x,480); assert.equal(context.dragRef.current,null);
  } finally { Object.assign(globalThis,saved); delete globalThis.__handInputTest; }
});
