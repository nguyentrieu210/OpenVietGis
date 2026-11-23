/** A bounded explicit-region 3D Tiles 1.1 subset; WGS84 ellipsoid / ECEF. */
export type Region=[number,number,number,number,number,number];
export type Tile={region:Region;error:number;matrix:number[];uri?:string;children:Tile[]};
export const TILE_LIMIT=8,TRIANGLE_LIMIT=250000;
const I=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],D=Math.PI/180;
const uriPattern=/^(?:[A-Za-z0-9_-]+\/)*[A-Za-z0-9_-]+\.glb$/;
const requireValue=(ok:unknown)=>{if(!ok)throw Error("Bộ 3D Tiles không hợp lệ hoặc không được hỗ trợ")};
const num=(v:unknown,lo:number,hi:number):v is number=>typeof v==="number"&&Number.isFinite(v)&&v>=lo&&v<=hi;
const keys=(v:any,allowed:string[])=>v&&typeof v==="object"&&!Array.isArray(v)&&Object.keys(v).every(k=>allowed.includes(k));
const multiply=(a:number[],b:number[])=>Array.from({length:16},(_,q)=>{const c=Math.floor(q/4),r=q%4;return a[r]!*b[c*4]!+a[4+r]!*b[c*4+1]!+a[8+r]!*b[c*4+2]!+a[12+r]!*b[c*4+3]!});
export function parseTileset(v:unknown):Tile{
 const doc=v as any;requireValue(keys(doc,["asset","geometricError","root"])&&keys(doc.asset,["version","tilesetVersion"])&&doc.asset.version==="1.1"&&(doc.asset.tilesetVersion===undefined||typeof doc.asset.tilesetVersion==="string"&&/^[ -~]{0,160}$/.test(doc.asset.tilesetVersion))&&num(doc.geometricError,0,1e7)&&doc.root?.refine==="REPLACE");
 let count=0;const seen=new Set<string>();
 const walk=(node:any,parent:number[],parentError:number,parentRegion:Region|null,depth:number):Tile=>{
  requireValue(++count<=256&&depth<=8&&keys(node,["boundingVolume","geometricError","refine","transform","content","children"])&&[undefined,"REPLACE"].includes(node.refine)&&num(node.geometricError,0,parentError)&&keys(node.boundingVolume,["region"]));
  const r=node.boundingVolume.region as Region;
  requireValue(Array.isArray(r)&&r.length===6&&r.every(n=>num(n,-12000,100000))&&-Math.PI<=r[0]&&r[0]<=r[2]&&r[2]<=Math.PI&&r[2]-r[0]<=Math.PI&&-85*D<=r[1]&&r[1]<=r[3]&&r[3]<=85*D&&r[4]<=r[5]);
  if(parentRegion)requireValue([0,1,4].every(i=>r[i]!>=parentRegion[i]!)&&[2,3,5].every(i=>r[i]!<=parentRegion[i]!));
  const t=node.transform??I;requireValue(Array.isArray(t)&&t.length===16&&t.every((n:unknown)=>num(n,-1e7,1e7))&&t[3]===0&&t[7]===0&&t[11]===0&&t[15]===1);
  const matrix=multiply(parent,t);requireValue(matrix.every(n=>Number.isFinite(n)&&Math.abs(n)<=1e9));
  let uri:string|undefined;if(node.content!==undefined){requireValue(keys(node.content,["uri"])&&typeof node.content.uri==="string"&&node.content.uri.length<=180&&uriPattern.test(node.content.uri)&&!seen.has(node.content.uri));uri=node.content.uri;seen.add(uri!)}
  const children=node.children??[];requireValue(Array.isArray(children)&&(children.length||uri&&node.geometricError===0));
  return {region:r,error:node.geometricError,matrix,uri,children:children.map((child:any)=>walk(child,matrix,node.geometricError,r,depth+1))};
 };
 const result=walk(doc.root,I,doc.geometricError,null,0);requireValue(seen.size>0&&seen.size<=128);return result;
}
export type View={west:number;south:number;east:number;north:number;longitude:number;latitude:number;zoom:number};
export function selectTiles(root:Tile,view:View,priorityUri?:string):{tiles:Tile[];omitted:number}{
 const w=view.west*D,e=view.east*D,s=view.south*D,n=view.north*D;
 const visible=(r:Region)=>r[1]<=n&&r[3]>=s&&[-2*Math.PI,0,2*Math.PI].some(k=>r[0]+k<=e&&r[2]+k>=w);
 const selected:Tile[]=[];const ppm=512*2**view.zoom/(40075016.6856*Math.max(.08,Math.cos(view.latitude*D)));
 const walk=(tile:Tile)=>{if(!visible(tile.region))return;const scale=Math.max(...[0,4,8].map(i=>Math.hypot(...tile.matrix.slice(i,i+3))));
  if(tile.uri&&(!tile.children.length||tile.error*scale*ppm<=16)){selected.push(tile);return}
  for(const child of tile.children)walk(child);
 };
 walk(root);
 const distance=(t:Tile)=>Math.hypot(((t.region[0]+t.region[2])/2-view.longitude*D)*Math.cos(view.latitude*D),(t.region[1]+t.region[3])/2-view.latitude*D);
 selected.sort((a,b)=>Number(b.uri===priorityUri)-Number(a.uri===priorityUri)||distance(a)-distance(b)||a.uri!.localeCompare(b.uri!));
 return {tiles:selected.slice(0,TILE_LIMIT),omitted:Math.max(0,selected.length-TILE_LIMIT)};
}
/** ECEF metres -> longitude/latitude degrees + ellipsoidal height metres. */
export function ecefToGeographic(x:number,y:number,z:number):[number,number,number]{
 const a=6378137,e2=6.6943799901413165e-3,p=Math.hypot(x,y);
 requireValue([x,y,z].every(Number.isFinite)&&p>1000&&Math.hypot(x,y,z)>6e6&&Math.hypot(x,y,z)<7e6);
 let lat=Math.atan2(z,p*(1-e2));
 for(let i=0;i<8;i++){const N=a/Math.sqrt(1-e2*Math.sin(lat)**2);lat=Math.atan2(z+e2*N*Math.sin(lat),p)}
 const N=a/Math.sqrt(1-e2*Math.sin(lat)**2),height=p/Math.cos(lat)-N;
 requireValue(num(lat,-85*D,85*D)&&num(height,-12000,100000));
 return [Math.atan2(y,x)/D,lat/D,height];
}
/** glTF y-up -> z-up, then the cumulative tileset ECEF transform. */
export function tileVertex(tile:Tile,x:number,y:number,z:number):[number,number,number]{
 const m=tile.matrix,p=ecefToGeographic(m[0]!*x-m[4]!*z+m[8]!*y+m[12]!,m[1]!*x-m[5]!*z+m[9]!*y+m[13]!,m[2]!*x-m[6]!*z+m[10]!*y+m[14]!);
 const r=tile.region;requireValue(p[0]*D>=r[0]-1e-7&&p[0]*D<=r[2]+1e-7&&p[1]*D>=r[1]-1e-7&&p[1]*D<=r[3]+1e-7&&p[2]>=r[4]-.5&&p[2]<=r[5]+.5);
 return p;
}
