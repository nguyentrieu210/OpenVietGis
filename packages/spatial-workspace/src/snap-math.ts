export type XY=[number,number];
export type SnapKind="vertex"|"edge"|"midpoint"|"intersection"|"perpendicular";
export type SnapMode="both"|"smart"|SnapKind;
export type Segment={a:XY;b:XY;radius?:number};
export type ScreenSnap={point:XY;kind:SnapKind;distance:number};
const sub=(a:XY,b:XY):XY=>[a[0]-b[0],a[1]-b[1]];
const cross=(a:XY,b:XY)=>a[0]*b[1]-a[1]*b[0];
const dot=(a:XY,b:XY)=>a[0]*b[0]+a[1]*b[1];
const at=(a:XY,v:XY,t:number):XY=>[a[0]+v[0]*t,a[1]+v[1]*t];
const eps=1e-9;
export const snapLabel:Record<SnapKind,string>={vertex:"Đỉnh",edge:"Cạnh",midpoint:"Trung điểm",intersection:"Giao điểm",perpendicular:"Vuông góc"};
export function findScreenSnap(target:XY,radius:number,mode:SnapMode,vertices:XY[],segments:Segment[],origin?:XY,vertexRadii?:readonly number[]):ScreenSnap|null{
 if(!Number.isFinite(radius)||radius<=0)return null;
 let best:ScreenSnap|null=null;
 const bounded=(value:number|undefined)=>value===undefined?radius:Number.isFinite(value)&&value>0?Math.min(value,radius):0;
 const priority=(k:SnapKind)=>mode==="smart"?(k==="edge"?1:0):0;
 function check(point:XY,kind:SnapKind,limit=radius){const distance=Math.hypot(point[0]-target[0],point[1]-target[1]);if(limit<=0||distance>limit||!Number.isFinite(distance))return;if(!best||priority(kind)<priority(best.kind)||priority(kind)===priority(best.kind)&&(distance<best.distance-eps||Math.abs(distance-best.distance)<eps&&kind==="vertex"))best={point,kind,distance}}
 const has=(kind:SnapKind)=>mode==="smart"||mode===kind||mode==="both"&&(kind==="edge"||kind==="vertex");
 if(has("vertex"))for(let i=0;i<vertices.length;i++)check(vertices[i]!,"vertex",bounded(vertexRadii?.[i]));
 const nearby:Segment[]=[];
 for(const {a,b,radius:limit} of segments){
  const local=bounded(limit);if(local<=0)continue;
  if(Math.max(a[0],b[0])<target[0]-local||Math.min(a[0],b[0])>target[0]+local||Math.max(a[1],b[1])<target[1]-local||Math.min(a[1],b[1])>target[1]+local)continue;
  const v=sub(b,a),d=dot(v,v);if(d<eps)continue;
  if(has("midpoint"))check(at(a,v,.5),"midpoint",local);
  if(has("edge"))check(at(a,v,Math.max(0,Math.min(1,dot(sub(target,a),v)/d))),"edge",local);
  if(has("perpendicular")&&origin){const t=dot(sub(origin,a),v)/d;if(t>=0&&t<=1&&Math.hypot(origin[0]-at(a,v,t)[0],origin[1]-at(a,v,t)[1])>eps)check(at(a,v,t),"perpendicular",local)}
  if(has("intersection"))nearby.push({a,b,radius:local});
 }
 // Limit pairwise work near cursor; nearest candidate segments are considered first.
 if(has("intersection")){
  const distanceTo=(s:Segment)=>{const v=sub(s.b,s.a),t=Math.max(0,Math.min(1,dot(sub(target,s.a),v)/dot(v,v)));const p=at(s.a,v,t);return Math.hypot(target[0]-p[0],target[1]-p[1])};
  nearby.sort((a,b)=>distanceTo(a)-distanceTo(b));const local=nearby.slice(0,128);
  for(let i=0;i<local.length;i++)for(let j=i+1;j<local.length;j++){
   const a=local[i],b=local[j],v=sub(a.b,a.a),w=sub(b.b,b.a),den=cross(v,w);if(Math.abs(den)<eps)continue;
   const delta=sub(b.a,a.a),t=cross(delta,w)/den,u=cross(delta,v)/den;if(t<0||t>1||u<0||u>1)continue;
   // An adjacent segment's shared endpoint is already a vertex, not a crossing.
   if((t<eps||t>1-eps)&&(u<eps||u>1-eps))continue;check(at(a.a,v,t),"intersection",Math.min(bounded(a.radius),bounded(b.radius)));
  }
 }
 return best;
}
