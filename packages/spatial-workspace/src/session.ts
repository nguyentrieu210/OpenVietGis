export type SpatialMode = "gis" | "cad" | "3d";
export type ProjectScope = Readonly<{accountId:string;organizationId:string;projectId:string}>;
export type ModeCapability = {available:true}|{available:false;reason:string};
export type EditBlockers = {dirty:boolean;saving:boolean};
export type ModeLoadRequest = Readonly<{mode:SpatialMode;sequence:number;scope:ProjectScope}>;
export type WorkspaceSession = Readonly<{
 scope:ProjectScope;active:SpatialMode;pending:ModeLoadRequest|null;sequence:number;error:string|null;
}>;
export type ModeResult = {ready:true;capability:ModeCapability;blockers:EditBlockers}|{ready:false;reason:string};
function checkedScope(scope:ProjectScope):ProjectScope{
 for(const id of [scope.accountId,scope.organizationId,scope.projectId]){
  if(typeof id!=="string"||!id.trim())throw new Error("Missing project scope");
 }
 return Object.freeze({accountId:scope.accountId,organizationId:scope.organizationId,projectId:scope.projectId});
}
function sameScope(a:ProjectScope,b:ProjectScope):boolean{
 return a.accountId===b.accountId&&a.organizationId===b.organizationId&&a.projectId===b.projectId;
}
export function createSession(scope:ProjectScope):WorkspaceSession{
 return {scope:checkedScope(scope),active:"gis",pending:null,sequence:0,error:null};
}
export function requestMode(s:WorkspaceSession,mode:SpatialMode,capability:ModeCapability,blockers:EditBlockers):WorkspaceSession{
 if(!["gis","cad","3d"].includes(mode))throw new Error("Unknown workspace mode");
 if(mode===s.active)return s.pending||s.error?{...s,pending:null,error:null}:s;
 if(!capability.available)return {...s,pending:null,error:capability.reason};
 if(blockers.dirty||blockers.saving)return {...s,pending:null,error:"Hãy lưu hoặc hủy chỉnh sửa trước khi chuyển chế độ."};
 if(s.pending?.mode===mode)return s;
 const sequence=s.sequence+1;
 return {...s,sequence,pending:Object.freeze({mode,sequence,scope:s.scope}),error:null};
}
export function finishMode(s:WorkspaceSession,request:ModeLoadRequest,result:ModeResult):WorkspaceSession{
 const p=s.pending;
 if(!p||p.sequence!==request.sequence||p.mode!==request.mode||!sameScope(s.scope,request.scope))return s;
 if(!result.ready)return {...s,pending:null,error:result.reason};
 if(!result.capability.available)return {...s,pending:null,error:result.capability.reason};
 if(result.blockers.dirty||result.blockers.saving)return {...s,pending:null,error:"Hãy lưu hoặc hủy chỉnh sửa trước khi chuyển chế độ."};
 return {...s,active:p.mode,pending:null,error:null};
}
export function resetScope(s:WorkspaceSession,scope:ProjectScope):WorkspaceSession{
 const valid=checkedScope(scope);
 if(sameScope(s.scope,valid))return s;
 return {...createSession(valid),sequence:s.sequence+1};
}
