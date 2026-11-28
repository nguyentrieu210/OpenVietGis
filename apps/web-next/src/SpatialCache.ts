import {completeSnapshot} from "./spatial-snapshot-guard";
import type {SpatialLayer,SpatialFeature} from "@openvietgis/client-sdk";
import type {StudioRow} from "./studio-contract";
import {featureKey} from "./studio-contract";
import type {CacheScope,CacheEntry,CachedRow} from "./SpatialCacheStorage";
export type {CacheScope,CacheEntry} from "./SpatialCacheStorage";

export function cacheAuthorized(restricted:boolean,permissions:readonly string[],layer:SpatialLayer){
  return layer.lifecycle==="active"&&restricted===false&&permissions.includes("project.read")&&layer.capabilities.some(c=>c.name==="feature_read"&&c.available&&c.actor_authorized);
}
export function cacheMatches(e:CacheEntry,l:SpatialLayer){
  return e.layer.id===l.id&&e.layer.source_id===l.source_id&&e.layer.layer_revision===l.layer_revision&&e.layer.source.schema_revision===l.source.schema_revision&&e.layer.source.content_revision===l.source.content_revision&&completeSnapshot(e.rows,l.source.feature_count);
}
export function restoreRows(entry:CacheEntry,l:SpatialLayer,color:string):StudioRow[]{
  return entry.rows.map(r=>({key:featureKey(l.source_id,r.feature.feature_id),layer:l,feature:r.feature as SpatialFeature,geometry:r.geometry,color}));
}

type Command={kind:"read";scope:CacheScope;layerId:string}|{kind:"write";scope:CacheScope;layer:SpatialLayer;rows:CachedRow[]}|{kind:"clear";account:string;scope?:CacheScope};
let worker:Worker|null=null,disabled=false,sequence=0;
const jobs=new Map<number,{resolve:(value:unknown)=>void;timer:number}>();
function resetWorker(){
  worker?.terminate();worker=null;disabled=true;
  for(const j of jobs.values()){window.clearTimeout(j.timer);j.resolve(null);}
  jobs.clear();
}
function execute(command:Command):Promise<unknown>{
  if(disabled||typeof Worker==="undefined")return Promise.resolve(null);
  try{
    if(!worker){
      worker=new Worker(new URL("./SpatialCacheWorker.ts",import.meta.url),{type:"module"});
      worker.onmessage=e=>{const j=jobs.get(e.data.id);if(j){jobs.delete(e.data.id);window.clearTimeout(j.timer);j.resolve(e.data.value)}};
      worker.onerror=resetWorker;
      worker.onmessageerror=resetWorker;
    }
    const id=++sequence;
    return new Promise(resolve=>{
      const timer=window.setTimeout(()=>{jobs.delete(id);resolve(null)},15000);
      jobs.set(id,{resolve,timer});
      try{worker!.postMessage({id,command})}catch{resetWorker()}
    });
  }catch{resetWorker();return Promise.resolve(null)}
}
export async function readCache(scope:CacheScope,layerId:string):Promise<CacheEntry|null>{
  return await execute({kind:"read",scope,layerId}) as CacheEntry|null;
}
export async function writeCache(scope:CacheScope,layer:SpatialLayer,rows:StudioRow[]){
  if(rows.length>20000||!completeSnapshot(rows,layer.source.feature_count)||rows.some(r=>r.layer.id!==layer.id||r.layer.source_id!==layer.source_id||r.layer.source.content_revision!==layer.source.content_revision||r.layer.source.schema_revision!==layer.source.schema_revision))return;
  await execute({kind:"write",scope,layer,rows:rows.map(({feature,geometry})=>({feature,geometry}))});
}
export async function clearSpatialCache(account:string,scope?:CacheScope){
  const result=await execute({kind:"clear",account,scope});
  if(result!==true){
    // Cleanup remains available even when the worker could not start.
    const storage=await import("./SpatialCacheStorage");await storage.clearSpatialCache(account,scope);
  }
}
