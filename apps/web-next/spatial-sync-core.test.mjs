import assert from 'node:assert/strict';
import {test} from 'node:test';
import {revisionChain,parallelBounded} from './src/SpatialSyncCore.ts';
const c=(id,base,result,count=1,status='committed')=>({id,base_content_revision:base,resulting_content_revision:result,operation_count:count,status});
test('exact revision chain covers missed events in order',()=>{
  assert.deepEqual(revisionChain([c('b','r1','r2'),c('a','r0','r1')],'r0','r2').map(x=>x.id),['a','b']);
  assert.deepEqual(revisionChain([],'r0','r0'),[]);
});
test('incomplete, oversized, reverted and cyclic chains require full layer reload',()=>{
  assert.equal(revisionChain([c('b','r1','r2')],'r0','r2'),null);
  assert.equal(revisionChain([c('a','r0','r1',65)],'r0','r1'),null);
  assert.equal(revisionChain([c('a','r0','r1',1,'reverted')],'r0','r1'),null);
  assert.equal(revisionChain([c('a','r1','r1')],'r0','r1'),null);
});
test('initial and incremental reads respect concurrency bound',async()=>{
  let running=0,peak=0,done=0;
  await parallelBounded(Array.from({length:36},(_,i)=>i),3,async()=>{
    peak=Math.max(peak,++running);await new Promise(r=>setTimeout(r,2));running--;done++;
  });
  assert.equal(peak,3);assert.equal(done,36);
});

test('one selection / edit among 35000 points produces one render update',async()=>{
  const {renderDiff}=await import('./src/SpatialSyncCore.ts');
  const rows=Array.from({length:35000},(_,i)=>({id:String(i),geometry:{type:'Point',coordinates:[i,1]},properties:{selected:false,color:'#aaa'}}));
  const updated=rows.map((r,i)=>i===100?{...r,properties:{...r.properties,selected:true}}:r);
  const start=performance.now();const diff=renderDiff(rows,updated);
  assert.equal(diff.update.length,1);assert.equal(diff.update[0].id,'100');
  assert.equal(diff.add.length+diff.remove.length,0);
  // Selecting must keep the feature's other properties in the tile worker (point vanished on click).
  assert.equal(diff.update[0].removeAllProperties,undefined);assert.deepEqual(diff.update[0].removeProperties,[]);
  assert.deepEqual(diff.update[0].addOrUpdateProperties,[{key:'selected',value:true},{key:'color',value:'#aaa'}]);
  const dropped=renderDiff(rows.slice(0,1),[{...rows[0],properties:{selected:true}}]);
  assert.deepEqual(dropped.update[0].removeProperties,['color']);
  console.log('35k render diff ms:',(performance.now()-start).toFixed(1));
  assert.equal(renderDiff(rows,[...rows,rows[0]]),null);
  assert.equal(renderDiff([{...rows[0],id:'dup'},{...rows[1],id:'dup'}],[{...rows[0],id:'dup'},{...rows[1],id:'dup'}]),null);
});

test('render property fast path preserves serialized comparison semantics',async()=>{
  const {renderDiff}=await import('./src/SpatialSyncCore.ts');
  const variants=[null,{}, {a:1,b:2},{b:2,a:1},{a:undefined},{a:null},{a:NaN},{a:0},{a:-0},{a:{x:1}},{a:[1,2]},{a:[2,1]},Object.assign(Object.create(null),{a:1})];
  for(const a of variants)for(const b of variants){
    const prev={id:'f',geometry:{type:'Point',coordinates:[1,2]},properties:a};
    const next={...prev,properties:b};
    assert.equal(renderDiff([prev],[next]).update.length,JSON.stringify(a)===JSON.stringify(b)?0:1);
  }
});

test('realtime wakeups coalesce, preserve one pending rerun and stop cleanly',async()=>{
  const {createCoalescedWake}=await import('./src/SpatialSyncCore.ts');
  const callbacks=[];let runs=0,release;
  const gate=createCoalescedWake(async()=>{
    runs++;
    if(runs===1)await new Promise(r=>{release=r});
  },{set:(cb)=>{callbacks.push(cb);return cb},clear:t=>{const i=callbacks.indexOf(t);if(i>=0)callbacks.splice(i,1)}});
  for(let i=0;i<50;i++)gate.trigger();
  assert.equal(callbacks.length,1);
  callbacks.shift()();await new Promise(r=>setTimeout(r,0));
  assert.equal(runs,1);assert.equal(gate.isRunning(),true);
  for(let i=0;i<50;i++)gate.trigger();
  assert.equal(callbacks.length,0);
  release();await new Promise(r=>setTimeout(r,0));
  assert.equal(callbacks.length,1);
  callbacks.shift()();await new Promise(r=>setTimeout(r,0));
  assert.equal(runs,2);assert.equal(gate.isRunning(),false);
  gate.trigger();assert.equal(callbacks.length,1);gate.stop();assert.equal(callbacks.length,0);
  gate.trigger();assert.equal(callbacks.length,0);
});
