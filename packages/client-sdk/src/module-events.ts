import type {ApiClient} from "./http";

/** Local invalidation signal only. No record values or actor permissions are published. */
export type ModuleChangeNotice=Readonly<{organizationId:string;projectId:string;extensionKey:string;kind:"permissions"|"bindings"|"activation"}>;
type Listener=(notice:ModuleChangeNotice)=>void;
type Subscription={organizationId:string;projectId:string;extensionKey:string;listener:Listener};
const subscriptions=new WeakMap<ApiClient,Set<Subscription>>();

/** Events are limited to the authenticated client instance and exact project/module. */
export function subscribeModuleChanges(client:ApiClient,organizationId:string,projectId:string,extensionKey:string,listener:Listener):()=>void{
 let set=subscriptions.get(client);if(!set){set=new Set();subscriptions.set(client,set);}
 const subscription={organizationId,projectId,extensionKey,listener};set.add(subscription);
 return ()=>{set.delete(subscription);if(!set.size&&subscriptions.get(client)===set)subscriptions.delete(client);};
}
/** Successful writes emit a hint; subscribers must reread authoritative API state. */
export function notifyModuleChange(client:ApiClient,notice:ModuleChangeNotice):void{
 const event=Object.freeze({...notice});
 for(const s of [...(subscriptions.get(client)??[])])if(s.organizationId===event.organizationId&&s.projectId===event.projectId&&s.extensionKey===event.extensionKey){
  // A bad listener cannot undo a committed API response or disrupt other listeners.
  try{s.listener(event);}catch(error){console.error("Module change listener failed",error);}
 }
}

import {projectApiPath} from "./http";
export type ModuleChangePage={mode:"snapshot_required"|"delta";cursor:string;has_more:boolean;items:{sequence:string;extension_key:string;kind:ModuleChangeNotice["kind"]}[]};
export function replayModuleChanges(client:ApiClient,org:string,project:string,cursor?:string,signal?:AbortSignal){
 const query=new URLSearchParams({limit:"100"});if(cursor!==undefined)query.set("cursor",cursor);
 return client.request<ModuleChangePage>(projectApiPath(org,project)+"/extensions/change-events?"+query,{signal,cache:"no-store"});
}
/** Deliver one replay page; advance the cursor only after successful processing.
 * Duplicate hints are harmless because subscribers reread source-of-truth state. */
export async function consumeModuleChanges(client:ApiClient,org:string,project:string,page:ModuleChangePage,onSnapshot:()=>Promise<void>,onActivation:()=>Promise<void>){
 if(!page||typeof page.cursor!=="string"||!Array.isArray(page.items)||!["delta","snapshot_required"].includes(page.mode))throw Error("Invalid module change page");
 if(page.mode==="snapshot_required"){await onSnapshot();return page.cursor;}
 if(page.items.some(e=>e.kind==="activation"))await onActivation();
 const unique=new Map<string,ModuleChangeNotice>();
 for(const event of page.items){
  if(!["activation","bindings","permissions"].includes(event.kind)||typeof event.extension_key!=="string")throw Error("Invalid module change event");
  const notice={organizationId:org,projectId:project,extensionKey:event.extension_key,kind:event.kind};
  unique.set(event.extension_key+":"+event.kind,notice);
 }
 for(const notice of unique.values())notifyModuleChange(client,notice);
 return page.cursor;
}
