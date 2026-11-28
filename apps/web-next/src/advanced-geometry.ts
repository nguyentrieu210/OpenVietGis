import type {Geometry,Position} from "geojson";
import {draftError,validXY} from "./digitizing";

export type XY=[number,number];

export function vertexEditable(g?:Geometry|null):g is Extract<Geometry,{type:"LineString"|"MultiLineString"|"Polygon"|"MultiPolygon"}>{
 return !!g&&(g.type==="LineString"||g.type==="MultiLineString"||g.type==="Polygon"||g.type==="MultiPolygon");
}

export function segmentEditable(g?:Geometry|null):g is Extract<Geometry,{type:"LineString"|"Polygon"}>{
 return !!g&&(g.type==="LineString"||g.type==="Polygon");
}

export function geometryPartCount(g:Geometry):number{
 return g.type==="MultiLineString"?g.coordinates.length:g.type==="MultiPolygon"?g.coordinates.length:1;
}

export function geometryRingCount(g:Geometry,part=0):number{
 return g.type==="Polygon"?g.coordinates.length:g.type==="MultiPolygon"?g.coordinates[part]?.length??0:1;
}

export function geometryVertices(g:Geometry,part=0,ring=0):XY[]{
 if(g.type==="LineString")return g.coordinates as XY[];
 if(g.type==="MultiLineString")return (g.coordinates[part]??[]) as XY[];
 if(g.type==="Polygon")return (g.coordinates[ring]??[]).slice(0,-1) as XY[];
 if(g.type==="MultiPolygon")return (g.coordinates[part]?.[ring]??[]).slice(0,-1) as XY[];
 return [];
}

export function withGeometryVertices(g:Geometry,part:number,ring:number,points:XY[]):Geometry{
 if(!points.length)return g;
 if(g.type==="LineString")return {...g,coordinates:points};
 if(g.type==="MultiLineString")return {...g,coordinates:g.coordinates.map((line,index)=>index===part?points:line)};
 if(g.type==="Polygon")return {...g,coordinates:g.coordinates.map((r,index)=>index===ring?[...points,points[0]!]:r)};
 if(g.type==="MultiPolygon")return {...g,coordinates:g.coordinates.map((polygon,index)=>index===part?polygon.map((r,j)=>j===ring?[...points,points[0]!]:r):polygon)};
 return g;
}

const sameXY=(a:readonly number[],b:readonly number[])=>a.length===2&&b.length===2&&a[0]===b[0]&&a[1]===b[1];
function polygonRings(g:Geometry,part:number):Position[][] {
 if(!Number.isInteger(part)||part<0)throw Error("Phần hình học không hợp lệ.");
 if(g.type==="Polygon"&&part===0)return g.coordinates;
 if(g.type==="MultiPolygon"&&g.coordinates[part])return g.coordinates[part]!;
 throw Error("Chọn một phần polygon.");
}
function replaceRings(g:Geometry,part:number,rings:Position[][]):Geometry{
 if(g.type==="Polygon")return {...g,coordinates:rings};
 if(g.type==="MultiPolygon")return {...g,coordinates:g.coordinates.map((p,i)=>i===part?rings:p)};
 throw Error("Chọn polygon.");
}
export function addPolygonHole(g:Geometry,part:number,points:XY[]):Geometry{
 const rings=polygonRings(g,part);
 if(points.length<3||points.length>512||points.some(p=>!validXY(p)))throw Error("Lỗ cần 3–512 đỉnh XY hợp lệ.");
 const open=sameXY(points[0]!,points.at(-1)!)?points.slice(0,-1):points;
 if(open.length<3)throw Error("Lỗ cần ít nhất ba đỉnh.");
 const closed=[...open.map(p=>[...p]),[...open[0]!]];
 const next=replaceRings(g,part,[...rings,closed]);
 const err=draftError(next);if(err)throw Error(err);
 // Reject boundary crossings and touching. PostGIS remains authoritative for
 // relationships with other polygons, nested holes and non-local geometries.
 const orient=(a:Position,b:Position,c:Position)=>(b[0]!-a[0]!)*(c[1]!-a[1]!)-(b[1]!-a[1]!)*(c[0]!-a[0]!);
 const on=(a:Position,b:Position,c:Position)=>orient(a,b,c)===0&&c[0]!>=Math.min(a[0]!,b[0]!)&&c[0]!<=Math.max(a[0]!,b[0]!)&&c[1]!>=Math.min(a[1]!,b[1]!)&&c[1]!<=Math.max(a[1]!,b[1]!);
 const intersects=(a:Position,b:Position,c:Position,d:Position)=>on(a,b,c)||on(a,b,d)||on(c,d,a)||on(c,d,b)||(orient(a,b,c)*orient(a,b,d)<0&&orient(c,d,a)*orient(c,d,b)<0);
 const inside=(p:Position,ring:Position[])=>{let hit=false;for(let i=0;i<ring.length-1;i++){const a=ring[i]!,b=ring[i+1]!;if(on(a,b,p))return false;if((a[1]!>p[1]!)!==(b[1]!>p[1]!)&&p[0]!<(b[0]!-a[0]!)*(p[1]!-a[1]!)/(b[1]!-a[1]!)+a[0]!)hit=!hit}return hit};
 const vertices=rings.reduce((n,r)=>n+r.length,0);
 if(vertices*closed.length>1_000_000||[...rings,closed].some(r=>r.some((p,i)=>i>0&&Math.abs(p[0]!-r[i-1]![0]!)>180)))throw Error("Vòng quá lớn hoặc qua kinh tuyến 180°.");
 if(!inside(closed[0]!,rings[0]!))throw Error("Lỗ phải nằm trong vành ngoài.");
 for(let k=0;k<rings.length;k++){
  const r=rings[k]!;
  if(k>0&&(inside(closed[0]!,r)||inside(r[0]!,closed)))throw Error("Lỗ chồng hoặc chứa lỗ khác.");
  for(let i=0;i<closed.length-1;i++)for(let j=0;j<r.length-1;j++)if(intersects(closed[i]!,closed[i+1]!,r[j]!,r[j+1]!))throw Error("Lỗ chạm hoặc cắt ranh giới.");
 }
 return next;
}
export function removePolygonHole(g:Geometry,part:number,ring:number):Geometry{
 const rings=polygonRings(g,part);
 if(!Number.isInteger(ring)||ring<=0||ring>=rings.length)throw Error("Chỉ được xóa lỗ, không xóa vành ngoài.");
 return replaceRings(g,part,rings.filter((_,i)=>i!==ring));
}
type SharedFeature={id:string;geometry:Geometry|null};
/** Exact coincident vertex propagation only. Require complete revision-pinned source
 * at the caller; do not claim tolerance snapping or segment noding. */
export function sharedVertexChanges(features:readonly SharedFeature[],selectedId:string,from:XY,to:XY){
 if(!validXY(from)||!validXY(to)||sameXY(from,to)||features.length<1||features.length>5000||new Set(features.map(f=>f.id)).size!==features.length)throw Error("Dữ liệu đỉnh chung không hợp lệ.");
 if(!features.some(f=>f.id===selectedId&&f.geometry&&vertexEditable(f.geometry)))throw Error("Không tìm thấy đối tượng đang sửa.");
 let seen=0;
 const changes:{id:string;geometry:Geometry}[]=[];
 for(const f of features){
  const g=f.geometry;if(!g)continue;
  let changed=false;
  const p=(v:Position):Position=>{if(++seen>200000)throw Error("Nguồn vượt giới hạn 200.000 đỉnh.");if(sameXY(v,from)){changed=true;return [...to]}return v};
  let next:Geometry;
  switch(g.type){
   case "LineString":next={...g,coordinates:g.coordinates.map(p)};break;
   case "MultiLineString":next={...g,coordinates:g.coordinates.map(r=>r.map(p))};break;
   case "Polygon":next={...g,coordinates:g.coordinates.map(r=>r.map(p))};break;
   case "MultiPolygon":next={...g,coordinates:g.coordinates.map(r=>r.map(s=>s.map(p)))};break;
   default:throw Error("Nguồn chứa kiểu hình học không hỗ trợ đỉnh chung.");
  }
  if(changed){const e=draftError(next);if(e)throw Error(f.id+": "+e);changes.push({id:f.id,geometry:next});if(changes.length>100)throw Error("Tối đa 100 đối tượng trong một thao tác đỉnh chung.");}
 }
 if(!changes.some(f=>f.id===selectedId))throw Error("Đỉnh nguồn không khớp đối tượng đã chọn.");
 return changes;
}

/** Add/remove parts without changing the declared multipart geometry family. */
export function addGeometryPart(g:Geometry,points:XY[]):Geometry{
 if(points.length>512||points.some(p=>!validXY(p)))throw Error("Phần mới vượt giới hạn hoặc tọa độ sai.");
 let next:Geometry;
 if(g.type==="MultiLineString"){
  if(points.length<2)throw Error("Tuyến mới cần ít nhất hai đỉnh.");
  next={...g,coordinates:[...g.coordinates,points.map(p=>[...p])]};
 }else if(g.type==="MultiPolygon"){
  const open=points.length&&sameXY(points[0]!,points.at(-1)!)?points.slice(0,-1):points;
  if(open.length<3)throw Error("Vùng mới cần ít nhất ba đỉnh.");
  next={...g,coordinates:[...g.coordinates,[[...open.map(p=>[...p]),[...open[0]!]]]]};
 }else throw Error("Chỉ thêm phần vào hình học multipart; không đổi loại nguồn.");
 const e=draftError(next);if(e)throw Error(e);return next;
}
export function removeGeometryPart(g:Geometry,part:number):Geometry{
 if(g.type!=="MultiLineString"&&g.type!=="MultiPolygon")throw Error("Chọn hình học multipart.");
 if(!Number.isInteger(part)||part<0||part>=g.coordinates.length||g.coordinates.length<=1)throw Error("Không được xóa phần cuối cùng.");
 return g.type==="MultiLineString"?{...g,coordinates:g.coordinates.filter((_,i)=>i!==part)}:{...g,coordinates:g.coordinates.filter((_,i)=>i!==part)};
}
/** Replace the forward path between two existing vertices; endpoints are retained.
 * For closed rings, the other path is retained and the ring is reclosed. */
export function reshapeGeometryPath(g:Geometry,part:number,ring:number,start:number,end:number,interior:XY[]):Geometry{
 if(!vertexEditable(g)||!Number.isInteger(part)||part<0||part>=geometryPartCount(g)||!Number.isInteger(ring)||ring<0||ring>=geometryRingCount(g,part))throw Error("Phần hoặc vòng không hợp lệ.");
 const p=geometryVertices(g,part,ring),closed=g.type==="Polygon"||g.type==="MultiPolygon";
 if(!Number.isInteger(start)||!Number.isInteger(end)||start<0||end>=p.length||start>=end||interior.length>512||interior.some(q=>!validXY(q)))throw Error("Chọn đỉnh đầu nhỏ hơn đỉnh cuối và tọa độ trung gian hợp lệ.");
 const next=[...p.slice(0,start+1),...interior,...p.slice(end)];
 if(next.length<(closed?3:2))throw Error("Hình học còn quá ít đỉnh.");
 const result=withGeometryVertices(g,part,ring,next),error=draftError(result);
 if(error)throw Error(error);return result;
}

/** Insert a node into all exactly matching edges of a complete pinned source.
 * Endpoint order does not matter. No tolerance or intersection inference. */
export function sharedEdgeNodeChanges(features:readonly SharedFeature[],selectedId:string,a:XY,b:XY){
 if(!validXY(a)||!validXY(b)||sameXY(a,b)||Math.abs(a[0]-b[0])>180||features.length<1||features.length>5000||new Set(features.map(f=>f.id)).size!==features.length)throw Error("Cạnh chung không hợp lệ.");
 const mid:XY=[a[0]+(b[0]-a[0])/2,a[1]+(b[1]-a[1])/2];
 if(sameXY(a,mid)||sameXY(b,mid))throw Error("Cạnh quá ngắn để chèn đỉnh.");
 let count=0;const changes:{id:string;geometry:Geometry}[]=[];
 for(const f of features){
  const g=f.geometry;if(!g)throw Error("Nguồn có hình học chưa xác minh.");
  let changed=false;
  const insert=(line:Position[])=>{
   count+=line.length;if(count>200000)throw Error("Nguồn vượt 200.000 đỉnh.");
   const result:Position[]=[];
   for(let i=0;i<line.length;i++){
    const p=line[i]!,q=line[i+1];result.push(p);
    if(q&&((sameXY(p,a)&&sameXY(q,b))||(sameXY(p,b)&&sameXY(q,a)))){result.push([...mid]);changed=true}
   }
   return result;
  };
  let next:Geometry;
  switch(g.type){
   case "LineString":next={...g,coordinates:insert(g.coordinates)};break;
   case "MultiLineString":next={...g,coordinates:g.coordinates.map(insert)};break;
   case "Polygon":next={...g,coordinates:g.coordinates.map(insert)};break;
   case "MultiPolygon":next={...g,coordinates:g.coordinates.map(p=>p.map(insert))};break;
   default:throw Error("Kiểu hình học không hỗ trợ cạnh chung.");
  }
  if(changed){const e=draftError(next);if(e)throw Error(f.id+": "+e);changes.push({id:f.id,geometry:next});if(changes.length>100)throw Error("Tối đa 100 đối tượng mỗi changeset.")}
 }
 if(!changes.some(c=>c.id===selectedId))throw Error("Cạnh không thuộc đối tượng đã chọn.");
 return changes;
}
