import type {ProjectAccessScope} from "./project-access-flight";

const CHANNEL="ovg-project-access-v1";
const STORAGE_KEY="__ovg_project_access_v1__";
export type ProjectAccessSignal={version:1;scope:ProjectAccessScope;nonce:string};

export function validProjectAccessSignal(value:unknown):value is ProjectAccessSignal{
 if(!value||typeof value!=="object")return false;
 const v=value as Partial<ProjectAccessSignal>,s=v.scope as Partial<ProjectAccessScope>|undefined;
 return v.version===1&&typeof v.nonce==="string"&&v.nonce.length>=8&&v.nonce.length<=128&&
  !!s&&typeof s.account==="string"&&s.account.length>0&&s.account.length<=128&&
  typeof s.org==="string"&&s.org.length>0&&s.org.length<=128&&
  typeof s.project==="string"&&s.project.length>0&&s.project.length<=128;
}

function signal(scope:ProjectAccessScope):ProjectAccessSignal{
 return {version:1,scope,nonce:crypto.randomUUID()};
}

/** Notify other tabs only; the caller must invalidate its own state directly. */
export function publishProjectAccessInvalidation(scope:ProjectAccessScope){
 const value=signal(scope);
 try{
  if(typeof BroadcastChannel!=="undefined"){
   const channel=new BroadcastChannel(CHANNEL);channel.postMessage(value);channel.close();return;
  }
 }catch{/* local fallback below */}
 try{
  localStorage.setItem(STORAGE_KEY,JSON.stringify(value));
  localStorage.removeItem(STORAGE_KEY);
 }catch{/* storage may be unavailable */}
}

export function subscribeProjectAccessInvalidation(handler:(scope:ProjectAccessScope)=>void){
 let closed=false,channel:BroadcastChannel|null=null;
 const accept=(value:unknown)=>{if(!closed&&validProjectAccessSignal(value))handler(value.scope)};
 try{
  if(typeof BroadcastChannel!=="undefined"){
   channel=new BroadcastChannel(CHANNEL);channel.onmessage=e=>accept(e.data);
  }
 }catch{channel=null}
 const storage=(event:StorageEvent)=>{
  if(event.key!==STORAGE_KEY||!event.newValue)return;
  try{accept(JSON.parse(event.newValue))}catch{/* malformed cross-tab signal */}
 };
 window.addEventListener("storage",storage);
 return()=>{closed=true;window.removeEventListener("storage",storage);channel?.close()};
}
