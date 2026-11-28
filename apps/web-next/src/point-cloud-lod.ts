import type {LasCloud} from "./local-las";
import {loadRemoteEpt} from "./local-ept";
import {loadRemoteCopc} from "./load-laz";
export type RemoteCloudSource={url:string;crs:4326|3857;format:"EPT"|"COPC";maxDepth:number;initialDepth:number};
type Cloud=LasCloud&{datasetCount?:number;nodeCount?:number;complete?:boolean};
export type CloudCamera={longitude:number;latitude:number;zoom:number};
export function depthForZoom(zoom:number,maxDepth:number):number{
 if(!Number.isFinite(zoom)||!Number.isInteger(maxDepth)||maxDepth<0||maxDepth>12)throw Error("Invalid point cloud LOD");
 return Math.max(0,Math.min(maxDepth,Math.floor((zoom-3)/1.5)));
}
type FetchCloud=(source:RemoteCloudSource,depth:number,signal:AbortSignal)=>Promise<Cloud>;
export const fetchCloud:FetchCloud=(s,d,signal)=>s.format==="COPC"?loadRemoteCopc(s.url,s.crs,d,signal):loadRemoteEpt(s.url,s.crs,d,signal);
/** One active fetch, at most two cached snapshots and one rendered snapshot.
 * Old network work is aborted, stale results never replace the current cloud.
 * URLs, source data, and cache live only in this authorized scene session.
 */
export class PointCloudLod {
 private cache=new Map<number,Cloud>();
 private pending:AbortController|null=null;private timer:ReturnType<typeof setTimeout>|null=null;
 private active=true;private wanted=-1;private applied:number;private completeDepth:number|undefined;
 constructor(private source:RemoteCloudSource,initial:Cloud,private replace:(cloud:Cloud,depth:number)=>void,private status:(text:string)=>void,private load:FetchCloud=fetchCloud){
  this.applied=source.initialDepth;this.cache.set(source.initialDepth,initial);if(initial.complete)this.completeDepth=source.initialDepth;
 }
 update(camera:CloudCamera){
  if(!this.active)return;
  let depth=depthForZoom(camera.zoom,this.source.maxDepth);if(this.completeDepth!==undefined)depth=Math.min(depth,this.completeDepth);
  if(depth===this.wanted)return;
  this.wanted=depth;this.cancel();
  if(depth===this.applied){this.status("");return}
  const cached=this.cache.get(depth);if(cached){this.apply(cached,depth);return}
  this.timer=setTimeout(()=>{this.timer=null;void this.refresh(depth)},350);
 }
 private apply(cloud:Cloud,depth:number){
  if(!this.active||this.wanted!==depth)return;
  if(!cloud.points.length||cloud.points.length>100000)throw Error("Point cloud snapshot exceeds render budget");
  this.replace(cloud,depth);this.applied=depth;
  this.cache.delete(depth);this.cache.set(depth,cloud);
  while(this.cache.size>2){const first=this.cache.keys().next().value;if(first!==undefined)this.cache.delete(first)}
  if(cloud.complete)this.completeDepth=Math.min(this.completeDepth??depth,depth);
  this.status("LOD "+depth+" · "+cloud.points.length.toLocaleString("vi-VN")+" điểm");
 }
 private async refresh(depth:number){
  if(!this.active||this.wanted!==depth)return;
  const ctl=new AbortController();this.pending=ctl;const timeout=setTimeout(()=>ctl.abort(),30000);
  this.status("Đang tải LOD "+depth+"…");
  try{const cloud=await this.load(this.source,depth,ctl.signal);if(!ctl.signal.aborted&&this.pending===ctl)this.apply(cloud,depth)}
  catch(e){if(this.active&&this.pending===ctl&&!ctl.signal.aborted)this.status("LOD "+depth+": "+(e instanceof Error?e.message:String(e)))}
  finally{clearTimeout(timeout);if(this.pending===ctl){this.pending=null;if(ctl.signal.aborted&&this.active)this.status("LOD đã hủy hoặc quá thời gian")}}
 }
 private cancel(){if(this.timer!==null){clearTimeout(this.timer);this.timer=null}const p=this.pending;this.pending=null;p?.abort()}
 dispose(){this.active=false;this.cancel();this.cache.clear()}
}
