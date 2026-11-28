/** Local, bounded 2DM triangular/quad elevation mesh. No implicit CRS. */
export type MeshNode=readonly [number,number,number];
export type LocalMesh={nodes:MeshNode[];nodeIds:number[];triangles:number[];bounds:[number,number,number,number];center:[number,number];minHeight:number;maxHeight:number};
export const MESH_MAX_BYTES=16*1024*1024;
export const MESH_MAX_TRIANGLES=100000;
const MAX_NODES=100000,R=6378137;
export function parse2dm(text:string,crs:4326|3857):LocalMesh{
 if(!text.length||text.length>MESH_MAX_BYTES||crs!==4326&&crs!==3857)throw Error("Invalid mesh size or CRS");
 const nodes:MeshNode[]=[],indices=new Map<number,number>(),faces:{id:number;refs:number[]}[]=[],ids=new Set<number>();
 const integer=(s:string|undefined)=>{if(!s||!/^\d{1,15}$/.test(s))throw Error("Invalid 2DM identifier");const n=Number(s);if(!Number.isSafeInteger(n)||n<1)throw Error("Invalid 2DM identifier");return n};
 const number=(s:string|undefined)=>{if(!s||!/^[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[-+]?\d+)?$/i.test(s))throw Error("Invalid mesh coordinate");const n=Number(s);if(!Number.isFinite(n))throw Error("Non-finite mesh coordinate");return n};
 let header=false,west=Infinity,south=Infinity,east=-Infinity,north=-Infinity,minHeight=Infinity,maxHeight=-Infinity,faceCount=0;
 for(const raw of text.split(/\r?\n/)){
  const line=raw.trim();if(!line||line.startsWith("#")||line.startsWith("//"))continue;
  if(line.length>512)throw Error("2DM record too long");
  const p=line.split(/\s+/),tag=p[0];
  if(!header){if(tag!=="MESH2D"||p.length!==1)throw Error("Expected MESH2D header");header=true;continue}
  if(tag==="ND"){
   if(p.length!==5||nodes.length>=MAX_NODES)throw Error("Invalid or excessive mesh nodes");
   const id=integer(p[1]);if(indices.has(id))throw Error("Duplicate mesh node");
   let x=number(p[2]),y=number(p[3]);const z=number(p[4]);
   if(crs===3857){if(Math.abs(x)>Math.PI*R||Math.abs(y)>Math.PI*R)throw Error("Projected coordinate outside domain");x=x/R*180/Math.PI;y=Math.atan(Math.sinh(y/R))*180/Math.PI}
   if(Math.abs(x)>180||Math.abs(y)>85||z< -12000||z>100000)throw Error("Mesh coordinate outside supported domain");
   indices.set(id,nodes.length);nodes.push([x,y,z]);west=Math.min(west,x);south=Math.min(south,y);east=Math.max(east,x);north=Math.max(north,y);minHeight=Math.min(minHeight,z);maxHeight=Math.max(maxHeight,z);
  }else if(tag==="E3T"||tag==="E4Q"){
   const size=tag==="E3T"?3:4;if(p.length!==size+3)throw Error("Invalid 2DM element");
   const id=integer(p[1]),refs=p.slice(2,2+size).map(integer);if(ids.has(id)||new Set(refs).size!==size)throw Error("Duplicate or degenerate element");
   integer(p[size+2]);ids.add(id);faceCount+=size-2;if(faceCount>MESH_MAX_TRIANGLES)throw Error("Mesh triangle limit exceeded");faces.push({id,refs});
  }else throw Error("Unsupported 2DM record: "+tag);
 }
 if(!header||!nodes.length||!faces.length||east-west>180)throw Error("Empty mesh or antimeridian crossing");
 const triangles:number[]=[],seen=new Set<string>();
 const cross=(a:MeshNode,b:MeshNode,c:MeshNode)=>(b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]);
 const add=(a:number,b:number,c:number)=>{const area=cross(nodes[a]!,nodes[b]!,nodes[c]!);if(!Number.isFinite(area)||Math.abs(area)<1e-18)throw Error("Degenerate mesh triangle");const key=[a,b,c].sort((x,y)=>x-y).join(":");if(seen.has(key))throw Error("Duplicate mesh triangle");seen.add(key);triangles.push(a,...(area>0?[b,c]:[c,b]))};
 for(const face of faces){const v=face.refs.map(id=>{const i=indices.get(id);if(i===undefined)throw Error("Missing mesh node");return i});if(v.length===3)add(v[0]!,v[1]!,v[2]!);else{const n=v.map(i=>nodes[i]!);const turns=n.map((p,i)=>cross(p,n[(i+1)%4]!,n[(i+2)%4]!));if(!turns.every(t=>t>1e-18)&&!turns.every(t=>t< -1e-18))throw Error("Non-convex or degenerate quad");add(v[0]!,v[1]!,v[2]!);add(v[0]!,v[2]!,v[3]!)}}
 return {nodes,nodeIds:[...indices.keys()],triangles,bounds:[west,south,east,north],center:[(west+east)/2,(south+north)/2],minHeight,maxHeight};
}
