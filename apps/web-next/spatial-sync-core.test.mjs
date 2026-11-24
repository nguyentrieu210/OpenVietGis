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
  console.log('35k render diff ms:',(performance.now()-start).toFixed(1));
  assert.equal(renderDiff(rows,[...rows,rows[0]]),null);
});
