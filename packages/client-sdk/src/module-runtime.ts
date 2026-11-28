import {ApiClient,projectApiPath} from "./http";
export type ModulePermissionPolicy={permissions:string[]|null;revision:number};
export type ModuleLayerBindingInput={source_id:string;role:string;schema_revision:string};
export type ModuleLayerBinding=ModuleLayerBindingInput&{current_schema_revision:string;status:"ready"|"unavailable"|"schema_changed"};
export type ModuleLayerBindings={revision:number;configured:boolean;items:ModuleLayerBinding[]};
const base=(org:string,project:string,key:string)=>`${projectApiPath(org,project)}/extensions/${encodeURIComponent(key)}`;
export function getMyModulePolicy(client:ApiClient,org:string,project:string,key:string){return client.request<ModulePermissionPolicy>(`${base(org,project,key)}/permissions/self`);}
export function getModulePolicy(client:ApiClient,org:string,project:string,key:string,type:"human"|"service_principal",id:string){return client.request<ModulePermissionPolicy>(`${base(org,project,key)}/permissions/${type}/${encodeURIComponent(id)}`);}
export function replaceModulePolicy(client:ApiClient,org:string,project:string,key:string,type:"human"|"service_principal",id:string,policy:ModulePermissionPolicy){return client.request<ModulePermissionPolicy>(`${base(org,project,key)}/permissions/${type}/${encodeURIComponent(id)}`,{method:"PUT",body:JSON.stringify({permissions:policy.permissions,expected_revision:policy.revision})});}
export function getModuleBindings(client:ApiClient,org:string,project:string,key:string){return client.request<ModuleLayerBindings>(`${base(org,project,key)}/layer-bindings`);}
export function replaceModuleBindings(client:ApiClient,org:string,project:string,key:string,expected_revision:number,items:ModuleLayerBindingInput[]){return client.request<ModuleLayerBindings>(`${base(org,project,key)}/layer-bindings`,{method:"PUT",body:JSON.stringify({expected_revision,items})});}
