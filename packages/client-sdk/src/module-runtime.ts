import {ApiClient,projectApiPath} from "./http";
import {notifyModuleChange} from "./module-events";
export type ModulePermissionPolicy={permissions:string[]|null;revision:number};
export type ModuleLayerBindingInput={source_id:string;role:string;schema_revision:string};
export type ModuleLayerBinding=ModuleLayerBindingInput&{current_schema_revision:string;status:"ready"|"unavailable"|"schema_changed"};
export type ModuleLayerBindings={revision:number;configured:boolean;items:ModuleLayerBinding[]};
const base=(org:string,project:string,key:string)=>`${projectApiPath(org,project)}/extensions/${encodeURIComponent(key)}`;
export function getMyModulePolicy(client:ApiClient,org:string,project:string,key:string){return client.request<ModulePermissionPolicy>(`${base(org,project,key)}/permissions/self`);}
export function getModulePolicy(client:ApiClient,org:string,project:string,key:string,type:"human"|"service_principal",id:string){return client.request<ModulePermissionPolicy>(`${base(org,project,key)}/permissions/${type}/${encodeURIComponent(id)}`);}
export async function replaceModulePolicy(client:ApiClient,org:string,project:string,key:string,type:"human"|"service_principal",id:string,policy:ModulePermissionPolicy){const result=await client.request<ModulePermissionPolicy>(`${base(org,project,key)}/permissions/${type}/${encodeURIComponent(id)}`,{method:"PUT",body:JSON.stringify({permissions:policy.permissions,expected_revision:policy.revision})});notifyModuleChange(client,{organizationId:org,projectId:project,extensionKey:key,kind:"permissions"});return result;}
export function getModuleBindings(client:ApiClient,org:string,project:string,key:string){return client.request<ModuleLayerBindings>(`${base(org,project,key)}/layer-bindings`);}
export async function replaceModuleBindings(client:ApiClient,org:string,project:string,key:string,expected_revision:number,items:ModuleLayerBindingInput[]){const result=await client.request<ModuleLayerBindings>(`${base(org,project,key)}/layer-bindings`,{method:"PUT",body:JSON.stringify({expected_revision,items})});notifyModuleChange(client,{organizationId:org,projectId:project,extensionKey:key,kind:"bindings"});return result;}

/** Pure calculation only; never substitutes for persisted proposal preview/apply. */
export function dispatchModuleCommand<T>(client:ApiClient,org:string,project:string,key:string,command:string,payload:unknown){return client.request<{command_id:string;output:T}>(`${base(org,project,key)}/commands/${encodeURIComponent(command)}`,{method:"POST",body:JSON.stringify({payload})});}

export type ModuleRuntimeContract={extension_key:string;version:string;api_compatibility:string;layer_roles:{id:string;geometry_families:string[];required_fields:string[]}[];pure_commands:{id:string;required_permission:string}[]};
export type BoundSpatialSummary={role:string;source_count:number;total:number;geometry_present:number};
export function getModuleRuntimeContract(client:ApiClient,org:string,project:string,key:string){return client.request<ModuleRuntimeContract>(`${base(org,project,key)}/runtime-contract`);}
export function getBoundSpatialSummary(client:ApiClient,org:string,project:string,key:string,role:string){return client.request<BoundSpatialSummary>(`${base(org,project,key)}/spatial-summary/${encodeURIComponent(role)}`);}
