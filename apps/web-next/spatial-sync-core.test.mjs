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
