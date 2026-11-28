import {resolve} from "node:path";
import {pathToFileURL} from "node:url";
import {execFileSync} from "node:child_process";
import os from "node:os";

if(typeof globalThis.gc!=="function")throw Error("Run with node --expose-gc");
const root=resolve(process.argv[2]??".");
const coordinatorUrl=pathToFileURL(resolve(root,"packages/spatial-workspace/src/coordinator.ts")).href;
const {ModeCoordinator:RootModeCoordinator}=await import(coordinatorUrl);
const scope={accountId:"fixture-account",organizationId:"fixture-org",projectId:"fixture-project"};
let activated=0,disposed=0,active=0,maxActive=0,serial=0;
const host={
 capability(){return {available:true}},
 blockers(){return {dirty:false,saving:false}},
 onChange(){},
 async prepare(mode,_scope,signal){
   if(signal.aborted)throw new DOMException("Aborted","AbortError");
   const id=++serial;
   let graph=Array.from({length:8192},(_,i)=>({id:i,mode,serial:id}));
   let isActive=false,isDisposed=false;
   return {
     activate(){
       if(isDisposed)throw Error("activated after disposal");
       if(!isActive){isActive=true;activated++;active++;maxActive=Math.max(maxActive,active)}
     },
     dispose(){
       if(isDisposed)throw Error("double dispose");
       isDisposed=true;disposed++;graph=[];
       if(isActive){isActive=false;active--}
     }
   };
 }
};
const coordinator=new RootModeCoordinator(scope,host);
const memory=()=>{globalThis.gc();const m=process.memoryUsage();return {heapUsed:m.heapUsed,arrayBuffers:m.arrayBuffers,rss:m.rss}};
const baseline=memory(),phases=[];
const cyclesPerPhase=1000,phaseCount=5;
for(let phase=0;phase<phaseCount;phase++){
 for(let i=0;i<cyclesPerPhase;i++){
   await coordinator.switchTo((phase*cyclesPerPhase+i)%2?"cad":"3d");
   if(active!==1)throw Error("post-switch active engine invariant failed");
 }
 phases.push(memory());
}
coordinator.dispose();
const final=memory();
const total=cyclesPerPhase*phaseCount;
if(activated!==total||disposed!==total||active!==0||maxActive>2)throw Error(JSON.stringify({activated,disposed,active,maxActive,total}));
const heapDeltas=phases.map(m=>m.heapUsed-baseline.heapUsed);
const retained=final.heapUsed-baseline.heapUsed;
if(Math.max(...heapDeltas)>12*1024*1024||retained>8*1024*1024)throw Error("retained lifecycle heap exceeded bound");
const sourceSha=execFileSync("git",["-C",root,"rev-parse","HEAD"],{encoding:"utf8"}).trim();
console.log(JSON.stringify({
 source_sha:sourceSha,root,node:process.version,platform:process.platform,arch:process.arch,os_release:os.release(),
 total_switches:total,activated,disposed,max_active_engines:maxActive,
 baseline,phases,final,phase_heap_delta_bytes:heapDeltas,final_heap_delta_bytes:retained,
 method:"ModeCoordinator soak with 5 phases x 1000 sequential CAD/3D switches; each prepared engine allocates 8192 object records; explicit GC at phase boundaries; fails on double/missed disposal, !=1 active engine after a completed switch, >2 during synchronous handoff, >12MiB phase retained heap or >8MiB final retained heap."
},null,2));
