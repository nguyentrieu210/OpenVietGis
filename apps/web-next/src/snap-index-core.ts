import type {Geometry,Position} from "geojson";
import type {XY,Segment} from "@openvietgis/spatial-workspace/snap-math";
type Box=[number,number,number,number];
type Stride=2|4;
export type SnapRecords={vertices:Float64Array;segments:Float64Array};
type Packed={records:Float64Array;stride:Stride;order:Uint32Array;boxes:Float64Array;links:Int32Array};
export type PreparedSnapIndex={vertices:Packed;segments:Packed};
const intersects=(b:Box,x0:number,y0:number,x1:number,y1:number)=>x0<=b[2]&&x1>=b[0]&&y0<=b[3]&&y1>=b[1];
function recordBox(a:Float64Array,i:number,s:Stride):Box{
 const x=a[i*s],y=a[i*s+1];return s===2?[x,y,x,y]:[Math.min(x,a[i*s+2]),Math.min(y,a[i*s+3]),Math.max(x,a[i*s+2]),Math.max(y,a[i*s+3])];
}
export function projectSnapRecords(g:Geometry,project:(p:Position)=>XY|null,mercator:boolean):SnapRecords{
 const vertices:number[]=[],segments:number[]=[];
 const line=(points:Position[])=>{let prev:XY|undefined;for(const p of points){const a=project(p);if(!a||!a.every(Number.isFinite)){prev=undefined;continue}vertices.push(a[0],a[1]);if(prev&&(!mercator||Math.abs(a[0]-prev[0])<20037508.342789244))segments.push(prev[0],prev[1],a[0],a[1]);prev=a}};
 if(g.type==="Point")line([g.coordinates]);else if(g.type==="MultiPoint")g.coordinates.forEach(p=>line([p]));else if(g.type==="LineString")line(g.coordinates);else if(g.type==="Polygon"||g.type==="MultiLineString")g.coordinates.forEach(line);else if(g.type==="MultiPolygon")g.coordinates.forEach(p=>p.forEach(line));
 return {vertices:new Float64Array(vertices),segments:new Float64Array(segments)};
}
// Packed bounding-box hierarchy: one spatial sort, then O(n) bottom-up boxes.
// Record order is retained independently to preserve deterministic snap ties.
function pack(records:Float64Array,stride:Stride):Packed{
 const n=records.length/stride,order=new Uint32Array(n);
 const bounds:Box=[Infinity,Infinity,-Infinity,-Infinity];
 const centersX=new Float64Array(n),centersY=new Float64Array(n);
 for(let i=0;i<n;i++){order[i]=i;const b=recordBox(records,i,stride);centersX[i]=(b[0]+b[2])/2;centersY[i]=(b[1]+b[3])/2;bounds[0]=Math.min(bounds[0],b[0]);bounds[1]=Math.min(bounds[1],b[1]);bounds[2]=Math.max(bounds[2],b[2]);bounds[3]=Math.max(bounds[3],b[3])}
 const dx=bounds[2]-bounds[0],dy=bounds[3]-bounds[1];
 // Quantized Morton ordering is only a packing heuristic; exact bounds are retained.
 const spread=(v:number)=>{let x=v&65535;x=(x|(x<<8))&0x00ff00ff;x=(x|(x<<4))&0x0f0f0f0f;x=(x|(x<<2))&0x33333333;x=(x|(x<<1))&0x55555555;return x};
 const morton=new Uint32Array(n);
 for(let i=0;i<n;i++){const x=dx>0?Math.floor(65535*(centersX[i]-bounds[0])/dx):0,y=dy>0?Math.floor(65535*(centersY[i]-bounds[1])/dy):0;morton[i]=(spread(x)|(spread(y)<<1))>>>0}
 order.sort((a,b)=>morton[a]-morton[b]||a-b);
 const boxes:number[]=[],links:number[]=[];
 const node=(start:number,end:number):number=>{
  const i=links.length/2;links.push(0,0);boxes.push(Infinity,Infinity,-Infinity,-Infinity);
  let b:Box=[Infinity,Infinity,-Infinity,-Infinity];
  const merge=(v:Box)=>{b=[Math.min(b[0],v[0]),Math.min(b[1],v[1]),Math.max(b[2],v[2]),Math.max(b[3],v[3])]};
  if(end-start<=16){links[2*i]=-start-1;links[2*i+1]=end;for(let k=start;k<end;k++)merge(recordBox(records,order[k],stride))}
  else{const mid=start+Math.floor((end-start)/2),l=node(start,mid),r=node(mid,end);links[2*i]=l;links[2*i+1]=r;for(const j of [l,r])merge([boxes[4*j],boxes[4*j+1],boxes[4*j+2],boxes[4*j+3]])}
  for(let j=0;j<4;j++)boxes[4*i+j]=b[j];return i;
 };
 if(n)node(0,n);
 return {records,stride,order,boxes:new Float64Array(boxes),links:new Int32Array(links)};
}
export function prepareSnapRecords(records:SnapRecords):PreparedSnapIndex{return {vertices:pack(records.vertices,2),segments:pack(records.segments,4)}}
function queryPacked(p:Packed,box:Box){
 const out:number[]=[],{records,stride,order,boxes,links}=p;
 function visit(i:number){if(!intersects(box,boxes[4*i],boxes[4*i+1],boxes[4*i+2],boxes[4*i+3]))return;const l=links[2*i],r=links[2*i+1];if(l<0){for(let k=-l-1;k<r;k++){const j=order[k],b=recordBox(records,j,stride);if(intersects(box,...b))out.push(j)}}else{visit(l);visit(r)}}
 if(links.length)visit(0);return out.sort((a,b)=>a-b);
}
function scan(records:Float64Array,stride:Stride,box:Box){
 const out:number[]=[];for(let i=0,n=records.length/stride;i<n;i++){const j=i*stride,x=records[j],y=records[j+1];if(stride===2?intersects(box,x,y,x,y):intersects(box,Math.min(x,records[j+2]),Math.min(y,records[j+3]),Math.max(x,records[j+2]),Math.max(y,records[j+3])))out.push(i)}return out;
}
const queryBox=(target:XY,radius:number):Box=>[target[0]-radius,target[1]-radius,target[0]+radius,target[1]+radius];
function materialize(v:Float64Array,s:Float64Array,vi:number[],si:number[]):{vertices:XY[];segments:Segment[]}{
 return {vertices:vi.map(i=>[v[i*2],v[i*2+1]]),segments:si.map(i=>({a:[s[i*4],s[i*4+1]],b:[s[i*4+2],s[i*4+3]]}))};
}
export function querySnapRecords(records:SnapRecords,target:XY,radius:number){const b=queryBox(target,radius);return materialize(records.vertices,records.segments,scan(records.vertices,2,b),scan(records.segments,4,b))}
export function queryPreparedSnapIndex(index:PreparedSnapIndex,target:XY,radius:number){const b=queryBox(target,radius);return materialize(index.vertices.records,index.segments.records,queryPacked(index.vertices,b),queryPacked(index.segments,b))}
