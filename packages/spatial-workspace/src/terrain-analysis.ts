import Delaunator from "delaunator";
import type {Feature,FeatureCollection,Geometry,Polygon} from "geojson";
import {geographic3857,project3857,terrainTriangles,validateTerrain,type TerrainGrid} from "./terrain.ts";

export type ElevationPoint=readonly [number,number,number];
const finite=(n:number)=>Number.isFinite(n);
const scale=(y:number)=>Math.cos(geographic3857(0,y)[1]*Math.PI/180);
const bounded=(n:number,lo:number,hi:number)=>Math.max(lo,Math.min(hi,n));
const range=(g:TerrainGrid)=>{const [l,b,r,t]=g.extent;return {l,b,r,t,dx:(r-l)/(g.width-1),dy:(t-b)/(g.height-1)}};
const node=(g:TerrainGrid,i:number):[number,number]=>{const {l,t,dx,dy}=range(g);return [l+i%g.width*dx,t-Math.floor(i/g.width)*dy]};
const area=(g:TerrainGrid,i:number)=>{const {dx,dy}=range(g);return dx*dy*scale(node(g,i)[1])**2};
export function elevationPoints(geometry:Geometry,z:number):ElevationPoint[]{
 if(!finite(z)||z< -12000||z>10000)throw Error("Cao độ mét ngoài miền hỗ trợ");
 const point=(p:number[]):ElevationPoint=>{if(p.length<2||!finite(p[0]!)||!finite(p[1]!)||Math.abs(p[0]!)>180||Math.abs(p[1]!)>85)throw Error("Tọa độ nguồn không hợp lệ");return [p[0]!,p[1]!,z]};
 if(geometry.type==="Point")return [point(geometry.coordinates)];
 if(geometry.type==="MultiPoint"||geometry.type==="LineString")return geometry.coordinates.map(point);
 if(geometry.type==="MultiLineString")return geometry.coordinates.flatMap(l=>l.map(point));
 throw Error("Chỉ nhận điểm cao độ hoặc đường bình độ");
}
export function interpolateTIN(points:readonly ElevationPoint[],maxEdgeM:number,referenceLabel:string,verticalReference:TerrainGrid["vertical_reference"]="local",resolution=96):TerrainGrid{
 if(points.length<3||points.length>8000||!finite(maxEdgeM)||maxEdgeM<1||maxEdgeM>100000||!referenceLabel.trim()||referenceLabel.length>120||!Number.isInteger(resolution)||resolution<16||resolution>128)throw Error("Cần 3–8.000 điểm, cạnh tối đa hợp lệ và mốc cao độ");
 const unique=new Map<string,[number,number,number]>();
 for(const [lon,lat,z] of points){if(![lon,lat,z].every(finite)||Math.abs(lon)>180||Math.abs(lat)>85||z< -12000||z>10000)throw Error("Điểm cao độ không hợp lệ");const [x,y]=project3857(lon,lat),key=x.toFixed(4)+","+y.toFixed(4),prev=unique.get(key);if(prev&&Math.abs(prev[2]-z)>.001)throw Error("Cùng tọa độ nhưng khác cao độ");unique.set(key,[x,y,z])}
 const p=[...unique.values()];if(p.length<3)throw Error("Thiếu điểm không trùng");
 const xs=p.map(v=>v[0]),ys=p.map(v=>v[1]),l=Math.min(...xs),r=Math.max(...xs),b=Math.min(...ys),t=Math.max(...ys),span=Math.max(r-l,t-b);
 if(r-l<.01||t-b<.01||span>100000)throw Error("Phạm vi TIN phải có diện tích và không quá 100 km Web Mercator");
 const width=Math.max(2,Math.ceil(resolution*(r-l)/span)),height=Math.max(2,Math.ceil(resolution*(t-b)/span)),dx=(r-l)/(width-1),dy=(t-b)/(height-1);
 const values:(number|null)[]=Array(width*height).fill(null);
 const tri=Delaunator.from(p,v=>v[0],v=>v[1]).triangles;
 const length=(a:number[],b:number[])=>Math.hypot(a[0]!-b[0]!,a[1]!-b[1]!)*scale((a[1]!+b[1]!)/2);
 let iterations=0;
 for(let i=0;i<tri.length;i+=3){
  const a=p[tri[i]!]!,b1=p[tri[i+1]!]!,c=p[tri[i+2]!]!;
  if(Math.max(length(a,b1),length(a,c),length(b1,c))>maxEdgeM)continue;
  const den=(b1[1]-c[1])*(a[0]-c[0])+(c[0]-b1[0])*(a[1]-c[1]);if(Math.abs(den)<1e-10)continue;
  const col0=bounded(Math.ceil((Math.min(a[0],b1[0],c[0])-l)/dx-1e-7),0,width-1),col1=bounded(Math.floor((Math.max(a[0],b1[0],c[0])-l)/dx+1e-7),0,width-1);
  const row0=bounded(Math.ceil((t-Math.max(a[1],b1[1],c[1]))/dy-1e-7),0,height-1),row1=bounded(Math.floor((t-Math.min(a[1],b1[1],c[1]))/dy+1e-7),0,height-1);
  for(let row=row0;row<=row1;row++)for(let col=col0;col<=col1;col++){
   if(++iterations>3_000_000)throw Error("TIN quá phức tạp; giảm điểm hoặc phạm vi");
   const x=l+col*dx,y=t-row*dy,u=((b1[1]-c[1])*(x-c[0])+(c[0]-b1[0])*(y-c[1]))/den,v=((c[1]-a[1])*(x-c[0])+(a[0]-c[0])*(y-c[1]))/den,w=1-u-v;
   if(Math.min(u,v,w)>=-1e-7)values[row*width+col]=bounded(a[2]*u+b1[2]*v+c[2]*w,Math.min(a[2],b1[2],c[2]),Math.max(a[2],b1[2],c[2]));
  }
 }
 const valid=values.filter((v):v is number=>v!==null);
 if(!valid.length)throw Error("Không đủ tam giác trong giới hạn cạnh đã chọn");
 const g=validateTerrain({source_id:"tin-preview",name:"TIN nội suy",crs_code:3857,source_crs_code:4326,width,height,extent:[l,b,r,t],values,min_m:Math.min(...valid),max_m:Math.max(...valid),valid_count:valid.length,nodata_count:values.length-valid.length,render_origin_m:Math.min(...valid),vertical_reference:verticalReference,reference_label:referenceLabel.trim(),elevation_unit:"metre",sampled:true,survey_grade:false,declared_by_user:true},"tin-preview");
 terrainTriangles(g);return g;
}
type P=[number,number,number];
// Integrate a piecewise-linear elevation difference over clipped triangle polygons.
function positiveVolume(tri:P[],s:number):number{
 const out:P[]=[];
 for(let j=0;j<tri.length;j++){const a=tri[j]!,b=tri[(j+1)%tri.length]!,za=a[2]*s,zb=b[2]*s;if(za>=0)out.push([a[0],a[1],za]);if((za>0&&zb<0)||(za<0&&zb>0)){const f=za/(za-zb);out.push([a[0]+(b[0]-a[0])*f,a[1]+(b[1]-a[1])*f,0])}}
 let result=0;
 for(let j=1;j+1<out.length;j++){const [a,b,c]=[out[0]!,out[j]!,out[j+1]!],ar=Math.abs((b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]))/2*scale((a[1]+b[1]+c[1])/3)**2;result+=ar*(a[2]+b[2]+c[2])/3}return result;
}
export function cutFill(g:TerrainGrid,level:number){
 if(!finite(level)||level< -12000||level>10000)throw Error("Mốc thiết kế không hợp lệ");
 const ids=terrainTriangles(g);let cut=0,fill=0,coveredArea=0;
 for(let i=0;i<ids.length;i+=3){const tri=Array.from(ids.slice(i,i+3),id=>[...node(g,id),g.values[id]!-level] as P);cut+=positiveVolume(tri,1);fill+=positiveVolume(tri,-1);coveredArea+=area(g,ids[i]!)/2}
 return {cut,fill,net:cut-fill,coveredArea};
}
export function terrainDerivatives(g:TerrainGrid){
 const {dx,dy}=range(g),slope:(number|null)[]=Array(g.values.length).fill(null),aspect:(number|null)[]=Array(g.values.length).fill(null);
 for(let y=1;y<g.height-1;y++)for(let x=1;x<g.width-1;x++){
  const i=y*g.width+x,[l,r,n,s]=[g.values[i-1],g.values[i+1],g.values[i-g.width],g.values[i+g.width]];
  if(g.values[i]==null||l==null||r==null||n==null||s==null)continue;
  const k=scale(node(g,i)[1]),gx=(r-l)/(2*dx*k),gy=(n-s)/(2*dy*k);
  slope[i]=Math.atan(Math.hypot(gx,gy))*180/Math.PI;if(Math.hypot(gx,gy)>1e-10)aspect[i]=(Math.atan2(-gx,-gy)*180/Math.PI+360)%360;
 }
 return {slope,aspect};
}
export function flowD8(g:TerrainGrid){
 const {dx,dy}=range(g),n=g.values.length,down=new Int32Array(n).fill(-1),acc=new Float64Array(n),order:number[]=[];
 for(let i=0;i<n;i++){const z=g.values[i];if(z==null)continue;const x=i%g.width,y=Math.floor(i/g.width);acc[i]=area(g,i)*(x===0||x===g.width-1?.5:1)*(y===0||y===g.height-1?.5:1);order.push(i);let best=0;
  for(let oy=-1;oy<=1;oy++)for(let ox=-1;ox<=1;ox++){if(!ox&&!oy||x+ox<0||x+ox>=g.width||y+oy<0||y+oy>=g.height)continue;const j=i+oy*g.width+ox,v=g.values[j];if(v==null)continue;const gradient=(z-v)/Math.hypot(ox*dx,oy*dy);if(gradient>best){best=gradient;down[i]=j}}
 }
 order.sort((a,b)=>g.values[b]!-g.values[a]!||a-b);
 for(const i of order){const j=down[i]!;if(j>=0)acc[j]!+=acc[i]!}
 return {down,acc,sinks:order.filter(i=>down[i]===-1).length};
}
export function contours(g:TerrainGrid,interval:number):FeatureCollection{
 if(!finite(interval)||interval<.001||Math.ceil((g.max_m-g.min_m)/interval)>120)throw Error("Khoảng cao đều phải từ 0,001 m và không quá 120 mức");
 const ids=terrainTriangles(g),features:Feature[]=[];
 for(let i=0;i<ids.length;i+=3){
  const ix=ids.slice(i,i+3),zs=ix.map(j=>g.values[j]!),lo=Math.min(...zs),hi=Math.max(...zs);if(hi===lo)continue;
  for(let k=Math.ceil(lo/interval);k*interval<hi;k++){const z=k*interval,pts:number[][]=[];
   for(let j=0;j<3;j++){const a=ix[j]!,b=ix[(j+1)%3]!,za=g.values[a]!,zb=g.values[b]!;if(z<Math.min(za,zb)||z>=Math.max(za,zb))continue;const u=(z-za)/(zb-za),p=node(g,a),q=node(g,b);pts.push(geographic3857(p[0]+u*(q[0]-p[0]),p[1]+u*(q[1]-p[1])))}
   if(pts.length===2&&Math.hypot(pts[0]![0]!-pts[1]![0]!,pts[0]![1]!-pts[1]![1]!)>1e-12){features.push({type:"Feature",geometry:{type:"LineString",coordinates:pts},properties:{elevation_m:z,color:"#80552d"}});if(features.length>40000)throw Error("Quá nhiều đoạn đồng mức; tăng khoảng cao đều")}
  }
 }
 return {type:"FeatureCollection",features};
}
export type AnalysisMode="slope"|"aspect"|"cutfill"|"flood"|"flow"|"elevation";
export function analysisFeatures(g:TerrainGrid,mode:AnalysisMode,level:number):FeatureCollection{
 const d=mode==="slope"||mode==="aspect"?terrainDerivatives(g):null,flow=mode==="flow"?flowD8(g):null,features:Feature[]=[];
 const {dx,dy}=range(g);
 if(mode==="flow"&&flow){for(let i=0;i<g.values.length;i++){const j=flow.down[i]!;if(j<0)continue;const v=flow.acc[i]!;features.push({type:"Feature",geometry:{type:"LineString",coordinates:[geographic3857(...node(g,i)),geographic3857(...node(g,j))]},properties:{upstream_area_m2:v,color:"#2374a5",width:bounded(Math.log10(Math.max(1,v)),1,5)}})}return {type:"FeatureCollection",features}}
 for(let y=0;y<g.height-1;y++)for(let x=0;x<g.width-1;x++){
  const i=y*g.width+x,ids=[i,i+1,i+g.width,i+g.width+1];if(ids.some(j=>g.values[j]==null))continue;
  const z=ids.reduce((sum,j)=>sum+g.values[j]!,0)/4;let value=z,color="#59864d";
  if(mode==="slope"){const v=d!.slope[i];if(v==null)continue;value=v;color=v<5?"#5eae71":v<15?"#dfcf58":v<30?"#e79643":"#bf4b39"}
  else if(mode==="aspect"){const v=d!.aspect[i];if(v==null)continue;value=v;color=["#356aac","#66a6c5","#78b86f","#c0c75b","#df9d52","#c86f61","#a16cab","#6f79bd"][Math.floor((v+22.5)%360/45)]!}
  else if(mode==="cutfill"){value=z-level;color=value>0?"#c45346":"#3787bf"}
  else if(mode==="flood"){value=level-z;if(value<0)continue;color="#398cc7"}
  else {const u=bounded((z-g.min_m)/Math.max(.001,g.max_m-g.min_m),0,1);color=u<.25?"#568e5e":u<.5?"#a0b868":u<.75?"#ceaa70":"#927461"}
  const [a,b]=node(g,i),poly:Polygon={type:"Polygon",coordinates:[[geographic3857(a,b),geographic3857(a+dx,b),geographic3857(a+dx,b-dy),geographic3857(a,b-dy),geographic3857(a,b)]]};
  features.push({type:"Feature",geometry:poly,properties:{value,mode,color}});
 }
 return {type:"FeatureCollection",features};
}
/** Minimal uncompressed Float32 GeoTIFF, PixelIsArea, EPSG:3857, explicit nodata. */
export function gridGeoTiff(g:TerrainGrid):ArrayBuffer{
 const {l,t,dx,dy}=range(g),nodata="-99999\0",entries=16,ifd=8,extra=ifd+2+entries*12+4,scaleAt=extra,tieAt=scaleAt+24,keysAt=tieAt+48,ndAt=keysAt+32,dataAt=(ndAt+nodata.length+3)&~3,bytes=g.values.length*4,buf=new ArrayBuffer(dataAt+bytes),v=new DataView(buf);
 v.setUint16(0,0x4949,true);v.setUint16(2,42,true);v.setUint32(4,ifd,true);v.setUint16(ifd,entries,true);
 const tags:[[number,number,number,number],... [number,number,number,number][]]=[[256,4,1,g.width],[257,4,1,g.height],[258,3,1,32],[259,3,1,1],[262,3,1,1],[273,4,1,dataAt],[277,3,1,1],[278,4,1,g.height],[279,4,1,bytes],[284,3,1,1],[339,3,1,3],[33550,12,3,scaleAt],[33922,12,6,tieAt],[34735,3,16,keysAt],[42113,2,nodata.length,ndAt],[274,3,1,1]];
 tags.sort((a,b)=>a[0]-b[0]).forEach(([tag,type,count,value],i)=>{const off=ifd+2+i*12;v.setUint16(off,tag,true);v.setUint16(off+2,type,true);v.setUint32(off+4,count,true);if(type===3&&count===1)v.setUint16(off+8,value,true);else v.setUint32(off+8,value,true)});
 [dx,dy,0].forEach((n,i)=>v.setFloat64(scaleAt+i*8,n,true));[0,0,0,l-dx/2,t+dy/2,0].forEach((n,i)=>v.setFloat64(tieAt+i*8,n,true));
 [1,1,0,3,1024,0,1,1,1025,0,1,1,3072,0,1,3857].forEach((n,i)=>v.setUint16(keysAt+i*2,n,true));
 for(let i=0;i<nodata.length;i++)v.setUint8(ndAt+i,nodata.charCodeAt(i));
 g.values.forEach((z,i)=>v.setFloat32(dataAt+i*4,z??-99999,true));return buf;
}

/** Difference only on exactly matching sampled grids and declared height reference. */
export function differenceTerrain(actual:TerrainGrid,design:TerrainGrid):TerrainGrid{
 if(actual.vertical_reference!==design.vertical_reference||actual.reference_label!==design.reference_label)throw Error("Hai DEM phải cùng mốc cao độ đã khai báo.");
 if(actual.width!==design.width||actual.height!==design.height||actual.extent.some((v,i)=>Math.abs(v-design.extent[i]!)>1e-6))throw Error("Hai DEM phải cùng lưới lấy mẫu; hãy căn chỉnh raster nguồn trước.");
 const values=actual.values.map((z,i)=>z==null||design.values[i]==null?null:z-design.values[i]!);
 const valid=values.filter((v):v is number=>v!==null);
 if(!valid.length)throw Error("Không có mẫu chồng phủ hợp lệ.");
 const g:TerrainGrid={...actual,source_id:"difference-preview",name:"Chênh lệch DEM",values,min_m:Math.min(...valid),max_m:Math.max(...valid),render_origin_m:Math.min(...valid),valid_count:valid.length,nodata_count:values.length-valid.length};
 terrainTriangles(g);return g;
}

/** Priority flood conditioning; outer boundary is open, NoData is an impermeable mask.
 * Disconnected valid islands with no outer-edge outlet are retained as closed basins.
 * Receiver links follow the earlier flood parent on flats; strict descent elsewhere.
 */
export function conditionedFlow(g:TerrainGrid){
 const n=g.values.length,w=g.width,h=g.height,filled=g.values.slice(),seen=new Uint8Array(n),rank=new Int32Array(n).fill(-1),parent=new Int32Array(n).fill(-1),heap:number[]=[],order:number[]=[];
 const less=(a:number,b:number)=>filled[a]!<filled[b]!||filled[a]===filled[b]&&a<b;
 const push=(i:number)=>{heap.push(i);let k=heap.length-1;while(k){const p=(k-1)>>1;if(!less(heap[k]!,heap[p]!))break;[heap[k],heap[p]]=[heap[p]!,heap[k]!];k=p}};
 const pop=()=>{const v=heap[0]!,last=heap.pop()!;if(heap.length){heap[0]=last;let k=0;while(true){let a=k*2+1;if(a>=heap.length)break;const b=a+1;if(b<heap.length&&less(heap[b]!,heap[a]!))a=b;if(!less(heap[a]!,heap[k]!))break;[heap[k],heap[a]]=[heap[a]!,heap[k]!];k=a}}return v};
 const neighbors=(i:number,fn:(j:number,ox:number,oy:number)=>void)=>{const x=i%w,y=Math.floor(i/w);for(let oy=-1;oy<=1;oy++)for(let ox=-1;ox<=1;ox++){if(!ox&&!oy||x+ox<0||x+ox>=w||y+oy<0||y+oy>=h)continue;const j=i+oy*w+ox;if(g.values[j]==null)continue;if(ox&&oy&&(g.values[i+ox]==null||g.values[i+oy*w]==null))continue;fn(j,ox,oy)}};
 for(let i=0;i<n;i++){const x=i%w,y=Math.floor(i/w);if(g.values[i]!=null&&(x===0||x===w-1||y===0||y===h-1)){seen[i]=1;push(i)}}
 while(heap.length){const i=pop();rank[i]=order.length;order.push(i);neighbors(i,j=>{if(seen[j])return;seen[j]=1;parent[j]=i;filled[j]=Math.max(g.values[j]!,filled[i]!);push(j)})}
 const down=new Int32Array(n).fill(-1),acc=new Float64Array(n),{dx,dy}=range(g);
 let raised=0,maxRaise=0,closed=0;
 for(let i=0;i<n;i++){if(g.values[i]==null)continue;if(!seen[i]){closed++;filled[i]=null;continue}const x=i%w,y=Math.floor(i/w);acc[i]=area(g,i)*(x===0||x===w-1?.5:1)*(y===0||y===h-1?.5:1);const dz=filled[i]!-g.values[i]!;if(dz>0){raised++;maxRaise=Math.max(maxRaise,dz)}let best=0;neighbors(i,(j,ox,oy)=>{if(rank[j]<0||rank[j]>=rank[i]!)return;const slope=(filled[i]!-filled[j]!)/Math.hypot(ox*dx,oy*dy);if(slope>best){best=slope;down[i]=j}});if(down[i]===-1&&parent[i]!==-1)down[i]=parent[i]!}
 for(let k=order.length-1;k>=0;k--){const i=order[k]!,j=down[i]!;if(j>=0)acc[j]!+=acc[i]!}
 return {filled,down,acc,raised,maxRaise,closed,outlets:order.filter(i=>down[i]===-1).length};
}
export function conditionedFlowFeatures(g:TerrainGrid):FeatureCollection{
 const f=conditionedFlow(g),features:Feature[]=[];
 for(let i=0;i<g.values.length;i++){const j=f.down[i]!;if(j<0)continue;const v=f.acc[i]!;features.push({type:"Feature",geometry:{type:"LineString",coordinates:[geographic3857(...node(g,i)),geographic3857(...node(g,j))]},properties:{upstream_area_m2:v,color:"#0c7890",width:bounded(Math.log10(Math.max(1,v)),1,5)}})}
 return {type:"FeatureCollection",features};
}
/** Boundary-connected low cells on the sampled grid; not a hydraulic model. */
export function connectedInundation(g:TerrainGrid,level:number):{features:FeatureCollection;connected:number;isolated:number}{
 if(!finite(level)||level< -12000||level>10000)throw Error("Mực nước không hợp lệ");
 const w=g.width-1,h=g.height-1,low=new Uint8Array(w*h),seen=new Uint8Array(w*h),queue:number[]=[],features:Feature[]=[];
 const {dx,dy}=range(g);
 for(let y=0;y<h;y++)for(let x=0;x<w;x++){const i=y*g.width+x,k=y*w+x,z=[g.values[i],g.values[i+1],g.values[i+g.width],g.values[i+g.width+1]];if(z.some(v=>v==null)||Math.max(...z as number[])>level)continue;low[k]=1;if(x===0||x===w-1||y===0||y===h-1){seen[k]=1;queue.push(k)}}
 for(let p=0;p<queue.length;p++){const k=queue[p]!,x=k%w,y=Math.floor(k/w);for(const [ox,oy] of [[1,0],[-1,0],[0,1],[0,-1]] as const){const nx=x+ox,ny=y+oy;if(nx<0||ny<0||nx>=w||ny>=h)continue;const j=ny*w+nx;if(low[j]&&!seen[j]){seen[j]=1;queue.push(j)}}}
 for(const k of queue){const x=k%w,y=Math.floor(k/w),i=y*g.width+x,[a,b]=node(g,i);features.push({type:"Feature",geometry:{type:"Polygon",coordinates:[[geographic3857(a,b),geographic3857(a+dx,b),geographic3857(a+dx,b-dy),geographic3857(a,b-dy),geographic3857(a,b)]]},properties:{color:"#267fbd",water_level_m:level,mode:"boundary-connected"}})}
 return {features:{type:"FeatureCollection",features},connected:queue.length,isolated:low.reduce((s,v)=>s+v,0)-queue.length};
}
