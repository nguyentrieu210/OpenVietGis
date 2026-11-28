// Coordinate editor for GeoJSON XY. Rings expose the closing coordinate once.
export type Path=number[];
export type Vertex={path:Path;xy:[number,number];closing?:Path};
type G=Exclude<GeoJSON.Geometry,GeoJSON.GeometryCollection>;
function at(o:unknown,path:Path):unknown{let v=o;for(const i of path){if(!Array.isArray(v)||i<0||i>=v.length)throw Error("INVALID_VERTEX");v=v[i]}return v}
function put(o:unknown,path:Path,value:[number,number]){const p=at(o,path.slice(0,-1));if(!Array.isArray(p)||!path.length)throw Error("INVALID_VERTEX");p[path.at(-1)!]=value}
function coord(v:unknown):[number,number]{if(!Array.isArray(v)||v.length!==2||v.some(n=>typeof n!=="number"||!Number.isFinite(n)))throw Error("INVALID_XY");return [v[0] as number,v[1] as number]}
export function vertices(g:G):Vertex[]{
 const out:Vertex[]=[];
 const line=(prefix:Path,ring=false)=>{
   const a=at(g.coordinates,prefix);
   if(!Array.isArray(a))throw Error("INVALID_GEOMETRY");
   const n=a.length-(ring?1:0);
   if(ring&&(a.length<4||JSON.stringify(a[0])!==JSON.stringify(a.at(-1))))throw Error("INVALID_RING");
   for(let i=0;i<n;i++)out.push({path:[...prefix,i],xy:coord(a[i]),...(ring&&i===0?{closing:[...prefix,a.length-1]}:{})});
 };
 switch(g.type){
  case "Point":out.push({path:[],xy:coord(g.coordinates)});break;
  case "MultiPoint":case "LineString":line([]);break;
  case "Polygon":g.coordinates.forEach((_,i)=>line([i],true));break;
  case "MultiLineString":g.coordinates.forEach((_,i)=>line([i]));break;
  case "MultiPolygon":g.coordinates.forEach((p,i)=>p.forEach((_,j)=>line([i,j],true)));break;
  default:throw Error("UNSUPPORTED_GEOMETRY");
 }
 if(out.length>256)throw Error("VERTEX_PREVIEW_LIMIT: use coordinate editor for larger geometries");
 return out;
}
export function moveVertex(g:G,v:Vertex,xy:[number,number]):G{
 if(xy.some(n=>!Number.isFinite(n))||Math.abs(xy[0])>180||Math.abs(xy[1])>90)throw Error("INVALID_XY");
 const next=structuredClone(g);
 if(!v.path.length&&next.type==="Point"){next.coordinates=[...xy];return next}
 put(next.coordinates,v.path,[...xy]);if(v.closing)put(next.coordinates,v.closing,[...xy]);
 return next;
}
