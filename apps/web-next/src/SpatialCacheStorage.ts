import {completeSnapshot} from "./spatial-snapshot-guard";
import type {SpatialLayer} from "@openvietgis/client-sdk";
import type {StudioRow} from "./studio-contract";

const DB="ovg-spatial-cache-v1", STORE="layers";
const MAX_AGE=24*60*60*1000, MAX_ROWS=20000, MAX_BYTES=24*1024*1024, TOTAL_BYTES=96*1024*1024;
export type CacheScope={account:string;org:string;project:string};
export type CachedRow=Pick<StudioRow,"feature"|"geometry">;
export type CacheEntry={key:string;scope:CacheScope;layer:SpatialLayer;rows:CachedRow[];saved:number;bytes:number;permissionKey?:string};
const key=(s:CacheScope,l:string)=>JSON.stringify([s.account,s.org,s.project,l]);
let opening:Promise<IDBDatabase>|undefined;
function db(){
  if(!opening)opening=new Promise<IDBDatabase>((resolve,reject)=>{
    const req=indexedDB.open(DB,1);
    req.onupgradeneeded=()=>req.result.createObjectStore(STORE,{keyPath:"key"});
    req.onsuccess=()=>{req.result.onversionchange=()=>{req.result.close();opening=undefined;};resolve(req.result)};
    req.onerror=()=>reject(req.error);
    req.onblocked=()=>reject(Error("Cache busy"));
  }).catch(e=>{opening=undefined;throw e});
  return opening;
}
export async function readCache(scope:CacheScope,layerId:string):Promise<CacheEntry|null>{
  try{
    const d=await db();
    return await new Promise((resolve,reject)=>{
      const r=d.transaction(STORE,"readonly").objectStore(STORE).get(key(scope,layerId));
      r.onsuccess=()=>{const e=r.result as CacheEntry|undefined;resolve(e&&e.key===key(scope,layerId)&&e.scope?.account===scope.account&&e.scope?.org===scope.org&&e.scope?.project===scope.project&&e.layer?.id===layerId&&Number.isFinite(e.saved)&&Date.now()>=e.saved&&Date.now()-e.saved<MAX_AGE&&Array.isArray(e.rows)&&e.rows.length<=MAX_ROWS&&Number.isFinite(e.bytes)&&e.bytes>=0&&e.bytes<=MAX_BYTES&&completeSnapshot(e.rows,e.layer.source.feature_count)?e:null)};
      r.onerror=()=>reject(r.error);
    });
  }catch{return null}
}
let queue:Promise<unknown>=Promise.resolve();
function serialized(fn:()=>Promise<void>){const task=queue.then(fn,fn);queue=task.catch(()=>{});return task.catch(()=>{})}
export function writeCache(scope:CacheScope,layer:SpatialLayer,rows:CachedRow[],permissionKey:string){
  if(!permissionKey||layer.organization_id!==scope.org||layer.project_id!==scope.project||!completeSnapshot(rows,layer.source.feature_count)||rows.length>MAX_ROWS)return Promise.resolve();
  return serialized(async()=>{
    const payload=rows;
    // Measure off the input event path. An oversized snapshot is never persisted.
    const bytes=new Blob([JSON.stringify(payload)]).size;
    if(bytes>MAX_BYTES)return;
    const entry:CacheEntry={key:key(scope,layer.id),scope,layer,rows:payload,saved:Date.now(),bytes,permissionKey};
    const d=await db();
    await new Promise<void>((resolve,reject)=>{
      const tx=d.transaction(STORE,"readwrite"),store=tx.objectStore(STORE),cursor=store.openCursor(),meta:Pick<CacheEntry,"key"|"saved"|"bytes">[]=[];
      cursor.onsuccess=()=>{
        const c=cursor.result;
        if(c){const {key,saved,bytes}=c.value as CacheEntry;meta.push({key,saved,bytes});c.continue();return;}
        let total=bytes;
        for(const e of meta.sort((a,b)=>b.saved-a.saved)){
          if(e.key===entry.key)continue;
          if(Date.now()-e.saved>=MAX_AGE||total+e.bytes>TOTAL_BYTES)store.delete(e.key);else total+=e.bytes;
        }
        store.put(entry);
      };
      tx.oncomplete=()=>resolve();tx.onerror=()=>reject(tx.error);tx.onabort=()=>reject(tx.error);
    });
  });
}
export function clearSpatialCache(account:string,scope?:CacheScope){
  return serialized(async()=>{
    const d=await db();
    await new Promise<void>((resolve,reject)=>{
      const tx=d.transaction(STORE,"readwrite"),store=tx.objectStore(STORE),r=store.openCursor();
      r.onsuccess=()=>{const c=r.result;if(!c)return;const e=c.value as CacheEntry;if(e.scope.account===account&&(!scope||(e.scope.org===scope.org&&e.scope.project===scope.project)))c.delete();c.continue()};
      tx.oncomplete=()=>resolve();tx.onerror=()=>reject(tx.error);
    });
  });
}
