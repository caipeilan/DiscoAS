import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import ts from 'typescript';
const source = ts.transpileModule(fs.readFileSync(new URL('../src/startupMotion.ts',import.meta.url),'utf8'), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const motion = await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);

test('startup easing reaches exact endpoints and remains monotonic with a faster middle',()=>{
  const progress = Array.from({length:101},(_,i)=>motion.startupProgress(i/100));
  assert.equal(progress[0],0); assert.equal(progress.at(-1),1);
  assert.ok(progress.every((p,i)=>Number.isFinite(p)&&p>=0&&p<=1&&(!i||p>=progress[i-1])));
  assert.ok(progress[51]-progress[50] > progress[1]-progress[0]);
  assert.equal(motion.startupProgress(-1),0); assert.equal(motion.startupProgress(2),1);
});

test('each entrance settles fully before the shared zoom and fade completes',()=>{
  const frames = motion.startupEntrance(-800,240);
  assert.equal(frames[0].opacity,0); assert.equal(frames.at(-1).opacity,1);
  assert.equal(frames.at(-1).transform,'translate(0px, 0px)');
  assert.ok(motion.STARTUP_EXIT_DELAY_MS > motion.STARTUP_ENTRY_MS + 2*motion.STARTUP_STAGGER_MS);
  const exit = motion.startupExit();
  assert.equal(exit[0].transform,'scale(1)'); assert.equal(exit.at(-1).opacity,0);
  assert.ok(Number(exit.at(-1).transform.slice(6,-1))>1);
  assert.ok(exit.every((f,i)=>!i||f.opacity<=exit[i-1].opacity));
  assert.ok(motion.startupExit(false).every(f=>f.transform==='scale(1)'));
});

test('entrances follow an arc without changing the specified starting and ending positions',()=>{
  for(const [x,y] of [[-850,320],[350,850],[350,-850]]){
    const frames=motion.startupEntrance(x,y);
    const point=frame=>frame.transform.match(/translate\(([^p]+)px, ([^p]+)px\)/).slice(1).map(Number);
    assert.deepEqual(point(frames[0]),[x,y]);assert.deepEqual(point(frames.at(-1)),[0,0]);
    const middle=point(frames[30]);
    assert.ok(Math.abs(x*middle[1]-y*middle[0])>1000);
    assert.ok(frames.every(frame=>point(frame).every(Number.isFinite)));
    assert.ok(motion.startupEntrance(0,0).every(frame=>point(frame).every(value=>value===0)));
  }
});

test('exit expansion follows a sine ease-out independently from opacity',()=>{
  const frames=motion.startupExit();
  const scale=frame=>Number(frame.transform.slice(6,-1));
  assert.equal(scale(frames[0]),1);
  assert.equal(scale(frames.at(-1)),1.62);
  const deltas=frames.slice(1).map((frame,index)=>scale(frame)-scale(frames[index]));
  assert.ok(deltas.every((delta,index)=>delta>=0&&(!index||delta<=deltas[index-1]+1e-7)));
  assert.ok(scale(frames[Math.floor(frames.length/2)])>1.4);
  assert.ok(motion.startupExit(false).every(frame=>scale(frame)===1));
});
