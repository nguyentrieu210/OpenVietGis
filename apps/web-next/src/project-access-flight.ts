import {projectApiPath,type ApiClient,type EffectivePermissions} from "@openvietgis/client-sdk";
export type ProjectAccessScope={account:string;org:string;project:string};
type Access={permissions:EffectivePermissions;access:{restricted:boolean}};
type Flight={controller:AbortController;promise:Promise<Access>};
const byClient=new WeakMap<ApiClient,Map<string,Flight>>();
const MAX_PENDING=32;
const scopeKey=(s:ProjectAccessScope)=>JSON.stringify([s.account,s.org,s.project]);

/** Coalesce concurrent checks only. Never retain settled authorization data. */
export function projectAccessFlight(client:ApiClient,scope:ProjectAccessScope):Promise<Access>{
 if(!scope.account||!scope.org||!scope.project)return Promise.reject(Error("Missing access scope"));
 let flights=byClient.get(client);
 if(!flights){flights=new Map();byClient.set(client,flights)}
 const key=scopeKey(scope),existing=flights.get(key);
 if(existing)return existing.promise;
 if(flights.size>=MAX_PENDING)return Promise.reject(Error("Too many pending authorization checks"));
 const controller=new AbortController(),signal=controller.signal;
 const query=new URLSearchParams({project_id:scope.project});
 const init={signal,cache:"no-store" as const};
 const timer=setTimeout(()=>controller.abort(),15000);
 const promise=Promise.all([
  client.request<EffectivePermissions>("/organizations/"+encodeURIComponent(scope.org)+"/effective-permissions?"+query,init),
  client.request<{restricted:boolean}>(projectApiPath(scope.org,scope.project)+"/access/mine",init)
 ]).then(([permissions,access])=>{
  if(signal.aborted)throw new DOMException("Authorization check cancelled","AbortError");
  if(!Array.isArray(permissions?.permissions)||!permissions.permissions.every(p=>typeof p==="string")||typeof access?.restricted!=="boolean")throw Error("Invalid project authorization response");
  return {permissions,access};
 }).finally(()=>{
  clearTimeout(timer);
  if(flights.get(key)?.controller===controller)flights.delete(key);
  controller.abort();
 });
 flights.set(key,{controller,promise});
 return promise;
}
/** On actor/session changes, cancel outstanding checks; never reuse their result. */
export function invalidateProjectAccess(client:ApiClient){
 const flights=byClient.get(client);
 if(!flights)return;
 for(const f of flights.values())f.controller.abort();
 flights.clear();
}
