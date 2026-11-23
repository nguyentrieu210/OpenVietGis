import {createSession,requestMode,finishMode,resetScope,type ProjectScope,type SpatialMode,type WorkspaceSession,type ModeCapability,type EditBlockers,type ModeLoadRequest} from "./session.ts";

/** Prepare is side-effect free with respect to the currently active canvas.
 * Activation attaches the prepared surface synchronously; dispose must be idempotent.
 * The GIS host owns its persistent map and camera independently of this controller.
 */
export interface PreparedEngine { activate():void; dispose():void }
export interface ModeHost {
 capability(mode:SpatialMode,scope:ProjectScope):ModeCapability;
 blockers():EditBlockers;
 prepare(mode:SpatialMode,scope:ProjectScope,signal:AbortSignal):Promise<PreparedEngine>;
 onChange(state:WorkspaceSession):void;
}
type Pending = {request:ModeLoadRequest;abort:AbortController};
export class ModeCoordinator {
 private session:WorkspaceSession;
 private loading:Pending|null=null;
 private activeEngine:PreparedEngine|null=null;
 private closed=false;
 private readonly host:ModeHost;
 constructor(scope:ProjectScope,host:ModeHost){this.host=host;this.session=createSession(scope)}
 get state(){return this.session}
 private publish(s:WorkspaceSession){if(s!==this.session){this.session=s;this.host.onChange(s)}}
 private cancel(){this.loading?.abort.abort();this.loading=null}
 private disposeEngine(engine:PreparedEngine|null){try{engine?.dispose()}catch{/* disposal is best-effort; authorization is enforced by host/server */}}
 private current(pending:Pending){return !this.closed&&this.loading===pending&&!pending.abort.signal.aborted&&this.session.pending===pending.request}
 async switchTo(mode:SpatialMode):Promise<void>{
  if(this.closed)return;
  const next=requestMode(this.session,mode,this.host.capability(mode,this.session.scope),this.host.blockers());
  if(next===this.session)return;
  this.cancel();
  if(!next.pending){this.publish(next);return}
  const pending:Pending={request:next.pending,abort:new AbortController()};
  this.loading=pending;
  this.publish(next);
  if(!this.current(pending))return;
  let prepared:PreparedEngine|null=null;
  try{
   prepared=await this.host.prepare(mode,pending.request.scope,pending.abort.signal);
   if(!this.current(pending)){this.disposeEngine(prepared);return}
   const result=finishMode(this.session,pending.request,{ready:true,capability:this.host.capability(mode,this.session.scope),blockers:this.host.blockers()});
   if(result.active!==mode){this.disposeEngine(prepared);this.loading=null;this.publish(result);return}
   prepared.activate();
   const previous=this.activeEngine;
   this.activeEngine=prepared;prepared=null;
   this.loading=null;
   this.disposeEngine(previous);
   this.publish(result);
  }catch{
   this.disposeEngine(prepared);
   if(!this.current(pending))return;
   this.loading=null;
   this.publish(finishMode(this.session,pending.request,{ready:false,reason:"Không thể mở chế độ. Vui lòng thử lại."}));
  }
 }
 /** Call when host authorization changes. Never keep an unauthorized adapter active. */
 revalidate(){
  if(this.closed)return;
  if(this.session.active!=="gis"&&!this.host.capability(this.session.active,this.session.scope).available){
   this.cancel();this.disposeEngine(this.activeEngine);this.activeEngine=null;
   this.publish({...this.session,active:"gis",pending:null,error:"Quyền truy cập chế độ đã thay đổi."});return;
  }
  if(this.session.pending&&!this.host.capability(this.session.pending.mode,this.session.scope).available){
   this.cancel();this.publish({...this.session,pending:null,error:"Quyền truy cập chế độ đã thay đổi."});
  }
 }
 updateScope(scope:ProjectScope){
  if(this.closed)return;
  const next=resetScope(this.session,scope);
  if(next===this.session)return;
  this.cancel();this.disposeEngine(this.activeEngine);this.activeEngine=null;this.publish(next);
 }
 dispose(){if(this.closed)return;this.closed=true;this.cancel();this.disposeEngine(this.activeEngine);this.activeEngine=null}
}
