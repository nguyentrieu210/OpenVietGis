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
