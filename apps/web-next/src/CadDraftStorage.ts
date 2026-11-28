import type {ProjectScope} from "@openvietgis/spatial-workspace/session";
import type {CadFileVersion} from "./CadEditorPanel";

const DB="ovg-cad-drafts-v1",STORE="drafts",MAX_BYTES=4*1024*1024,MAX_AGE=7*24*60*60*1000;
export type CadDraft={key:string;scope:ProjectScope;documentId:string;version:number;sha256:string;text:string;saved:number;bytes:number};
const key=(scope:ProjectScope,doc:Pick<CadFileVersion,"id"|"version">)=>JSON.stringify([scope.accountId,scope.organizationId,scope.projectId,doc.id,doc.version]);
let opening:Promise<IDBDatabase>|undefined;
function db(){
 if(!opening)opening=new Promise<IDBDatabase>((resolve,reject)=>{
  const r=indexedDB.open(DB,1);
  r.onupgradeneeded=()=>r.result.createObjectStore(STORE,{keyPath:"key"});
  r.onsuccess=()=>{r.result.onversionchange=()=>{r.result.close();opening=undefined};resolve(r.result)};
  r.onerror=()=>reject(r.error);r.onblocked=()=>reject(Error("CAD draft storage busy"));
 }).catch(e=>{opening=undefined;throw e});
 return opening;
}
function valid(entry:CadDraft|undefined,scope:ProjectScope,doc:Pick<CadFileVersion,"id"|"version"|"sha256">){
 return !!entry&&entry.key===key(scope,doc)&&entry.scope.accountId===scope.accountId&&entry.scope.organizationId===scope.organizationId&&entry.scope.projectId===scope.projectId&&
  entry.documentId===doc.id&&entry.version===doc.version&&entry.sha256===doc.sha256&&Date.now()-entry.saved<=MAX_AGE&&entry.bytes<=MAX_BYTES&&typeof entry.text==="string";
}
export async function readCadDraft(scope:ProjectScope,doc:Pick<CadFileVersion,"id"|"version"|"sha256">):Promise<CadDraft|null>{
 try{const d=await db();return await new Promise((resolve,reject)=>{const r=d.transaction(STORE,"readonly").objectStore(STORE).get(key(scope,doc));r.onsuccess=()=>resolve(valid(r.result as CadDraft|undefined,scope,doc)?r.result:null);r.onerror=()=>reject(r.error)})}catch{return null}
}
export async function writeCadDraft(scope:ProjectScope,doc:Pick<CadFileVersion,"id"|"version"|"sha256">,text:string){
 const bytes=new Blob([text]).size;if(!doc.id||doc.version<1||bytes>MAX_BYTES)return false;
 const entry:CadDraft={key:key(scope,doc),scope,documentId:doc.id,version:doc.version,sha256:doc.sha256,text,saved:Date.now(),bytes};
 try{const d=await db();await new Promise<void>((resolve,reject)=>{const tx=d.transaction(STORE,"readwrite");tx.objectStore(STORE).put(entry);tx.oncomplete=()=>resolve();tx.onerror=()=>reject(tx.error);tx.onabort=()=>reject(tx.error)});return true}catch{return false}
}
export async function deleteCadDraft(scope:ProjectScope,doc:Pick<CadFileVersion,"id"|"version">){
 try{const d=await db();await new Promise<void>((resolve,reject)=>{const tx=d.transaction(STORE,"readwrite");tx.objectStore(STORE).delete(key(scope,doc));tx.oncomplete=()=>resolve();tx.onerror=()=>reject(tx.error)})}catch{/* best effort */}
}
export async function findCadDraftDocument(scope:ProjectScope,docs:readonly CadFileVersion[]):Promise<CadFileVersion|null>{
 const byKey=new Map(docs.map(d=>[key(scope,d),d]));
 try{const d=await db();return await new Promise((resolve,reject)=>{
  const r=d.transaction(STORE,"readonly").objectStore(STORE).openCursor(),matches:{doc:CadFileVersion;saved:number}[]=[];
  r.onsuccess=()=>{const c=r.result;if(!c){matches.sort((a,b)=>b.saved-a.saved);resolve(matches[0]?.doc??null);return}const e=c.value as CadDraft,doc=byKey.get(e.key);if(doc&&valid(e,scope,doc))matches.push({doc,saved:e.saved});c.continue()};r.onerror=()=>reject(r.error)
 })}catch{return null}
}
export async function clearCadDrafts(accountId:string){
 try{const d=await db();await new Promise<void>((resolve,reject)=>{const tx=d.transaction(STORE,"readwrite"),r=tx.objectStore(STORE).openCursor();r.onsuccess=()=>{const c=r.result;if(!c)return;const e=c.value as CadDraft;if(e.scope.accountId===accountId)c.delete();c.continue()};tx.oncomplete=()=>resolve();tx.onerror=()=>reject(tx.error)})}catch{/* best effort */}
}
