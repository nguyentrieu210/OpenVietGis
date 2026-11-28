import type {Geometry,Position} from "geojson";
import type {XY} from "@openvietgis/spatial-workspace/snap-math";
import {projectSnapRecords,prepareSnapRecords,querySnapRecords,queryPreparedSnapIndex,type SnapRecords,type PreparedSnapIndex} from "./snap-index-core";

export function createSnapIndex(g:Geometry,project:(p:Position)=>XY|null,mercator:boolean){
 const index=prepareSnapRecords(projectSnapRecords(g,project,mercator));return (target:XY,radius:number)=>queryPreparedSnapIndex(index,target,radius);
}
type Cached={records?:SnapRecords;prepared?:PreparedSnapIndex;building?:boolean;failed?:boolean};
type MapCache={signature:string;indexes:WeakMap<Geometry,Cached>;mercator:WeakMap<Geometry,Cached>};
const cache=new WeakMap<object,MapCache>();
let workers=0;
function buildAsync(entry:Cached){
 if(!entry.records||entry.prepared||entry.building||entry.failed||workers>=2||typeof Worker==="undefined")return false;
 let worker:Worker;
 try{worker=new Worker(new URL("./snap-index.worker.ts",import.meta.url),{type:"module"})}catch{entry.failed=true;return false}
 workers++;entry.building=true;let finished=false;
 const done=()=>{if(finished)return;finished=true;workers--;entry.building=false;worker.terminate();clearTimeout(timer)};
 const fail=()=>{entry.failed=true;done()};
 const timer=setTimeout(fail,15000);
 worker.onmessage=e=>{if(e.data?.ok){entry.prepared=e.data.index;entry.records=undefined;done()}else fail()};
 worker.onerror=fail;
 try{worker.postMessage(entry.records)}catch{fail();return false}
 return true;
}
function entryFor(map:object,signature:string,g:Geometry,project:(p:Position)=>XY|null,mercator:boolean){
 let c=cache.get(map);if(!c){c={signature,indexes:new WeakMap(),mercator:new WeakMap()};cache.set(map,c)}else if(c.signature!==signature){c.signature=signature;c.indexes=new WeakMap()}
 const indexes=mercator?c.mercator:c.indexes;let entry=indexes.get(g);
 if(!entry){const records=projectSnapRecords(g,project,mercator);entry={records};indexes.set(g,entry);if(records.vertices.length<8192||typeof Worker==="undefined"){entry.prepared=prepareSnapRecords(records);entry.records=undefined}else buildAsync(entry)}
 return entry;
}
export function indexedSnapCandidates(map:object,signature:string,g:Geometry,project:(p:Position)=>XY|null,mercator:boolean,target:XY,radius:number){
 const entry=entryFor(map,signature,g,project,mercator);
 if(entry.prepared)return queryPreparedSnapIndex(entry.prepared,target,radius);
 // Correct cold-path scan while the bounded worker constructs the hierarchy.
 if(!entry.building)buildAsync(entry);
 return querySnapRecords(entry.records!,target,radius);
}
