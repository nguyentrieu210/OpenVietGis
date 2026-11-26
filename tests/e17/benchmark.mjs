import {spawnSync} from 'node:child_process';
import {performance} from 'node:perf_hooks';
import {pathToFileURL, fileURLToPath} from 'node:url';
import {resolve} from 'node:path';
import assert from 'node:assert/strict';
const root=resolve(process.argv[2] ?? '.');
const src=resolve(root,'apps/web-next/src');
const load=name=>import(pathToFileURL(resolve(src,name)).href);
const t0=performance.now();
const {readWorkspaceRoute}=await load('route-compat.ts');
const {renderDiff}=await load('SpatialSyncCore.ts');
const importMs=performance.now()-t0;
const rows=Array.from({length:35000},(_,i)=>({id:String(i),geometry:{type:'Point',coordinates:[106+i/1e6,11]},properties:{layerId:'L',selected:false}}));
const changed=rows.map((r,i)=>i===100?{...r,properties:{...r.properties,selected:true}}:r);
function routeBatch(){for(let i=0;i<1000;i++)assert.equal(readWorkspaceRoute(`/app/organizations/o/projects/p${i}/queries`).project,`p${i}`);}
function diff(){const d=renderDiff(rows,changed);assert.equal(d.update.length,1);assert.equal(d.update[0].id,'100');assert.equal(d.add.length+d.remove.length,0);}
if(process.argv[3]==='cold'){const t=performance.now();routeBatch();const routeMs=performance.now()-t;const s=performance.now();diff();console.log(JSON.stringify({importMs,routeMs,diffMs:performance.now()-s,peakRssKiB:process.resourceUsage().maxRSS}));}
else {
  const cold=[];
  for(let i=0;i<30;i++){const c=spawnSync(process.execPath,[fileURLToPath(import.meta.url),root,'cold'],{encoding:'utf8',timeout:30000});assert.equal(c.status,0,c.stderr||String(c.error));cold.push(JSON.parse(c.stdout));}
  function measure(fn,n){for(let i=0;i<5;i++)fn();return Array.from({length:n},()=>{const t=performance.now();fn();return performance.now()-t;});}
  const summary=a=>{const s=[...a].sort((x,y)=>x-y);const p=q=>s[Math.ceil(q*s.length)-1];return {n:s.length,p50:p(.5),p95:p(.95),p99:p(.99),raw:a};};
  console.log(JSON.stringify({node:process.version,platform:process.platform,arch:process.arch,scope:'JS module microbenchmark only; not browser/backend/network latency',errors:0,cold:{import:summary(cold.map(x=>x.importMs)),routeBatch:summary(cold.map(x=>x.routeMs)),renderDiff:summary(cold.map(x=>x.diffMs)),peakRssKiB:Math.max(...cold.map(x=>x.peakRssKiB))},warm:{routeBatch:summary(measure(routeBatch,100)),renderDiff:summary(measure(diff,100)),peakRssKiB:process.resourceUsage().maxRSS}},null,2));
}
