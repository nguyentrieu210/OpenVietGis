import type {Geometry,Position} from "geojson";
import type {XY,Segment} from "@openvietgis/spatial-workspace/snap-math";
type Box=[number,number,number,number];
type Entry<T>={box:Box;value:T;order:number};
type Node<T>={box:Box;items?:Entry<T>[];left?:Node<T>;right?:Node<T>};
const overlaps=(a:Box,b:Box)=>a[0]<=b[2]&&a[2]>=b[0]&&a[1]<=b[3]&&a[3]>=b[1];
function build<T>(items:Entry<T>[]):Node<T>|null{
 if(!items.length)return null;
 const box=items.reduce<Box>((b,e)=>[Math.min(b[0],e.box[0]),Math.min(b[1],e.box[1]),Math.max(b[2],e.box[2]),Math.max(b[3],e.box[3])],[Infinity,Infinity,-Infinity,-Infinity]);
 if(items.length<=16)return {box,items};
 const axis=box[2]-box[0]>=box[3]-box[1]?0:1;
 items.sort((a,b)=>(a.box[axis]+a.box[axis+2])-(b.box[axis]+b.box[axis+2]));
 const half=items.length>>1;return {box,left:build(items.slice(0,half))!,right:build(items.slice(half))!};
}
function query<T>(node:Node<T>|null,box:Box){
 const out:Entry<T>[]=[];function visit(n:Node<T>|null|undefined){if(!n||!overlaps(n.box,box))return;if(n.items){for(const e of n.items)if(overlaps(e.box,box))out.push(e)}else{visit(n.left);visit(n.right)}}visit(node);
 return out.sort((a,b)=>a.order-b.order).map(e=>e.value);
}
export function createSnapIndex(g:Geometry,project:(p:Position)=>XY|null,mercator:boolean){
 const vertices:Entry<XY>[]=[],segments:Entry<Segment>[]=[],line=(points:Position[])=>{let prev:XY|undefined;for(const p of points){const a=project(p);if(!a||!a.every(Number.isFinite)){prev=undefined;continue}vertices.push({box:[a[0],a[1],a[0],a[1]],value:a,order:vertices.length});if(prev&&(!mercator||Math.abs(a[0]-prev[0])<20037508.342789244)){segments.push({box:[Math.min(a[0],prev[0]),Math.min(a[1],prev[1]),Math.max(a[0],prev[0]),Math.max(a[1],prev[1])],value:{a:prev,b:a},order:segments.length})}prev=a}};
 if(g.type==="Point")line([g.coordinates]);else if(g.type==="MultiPoint")g.coordinates.forEach(p=>line([p]));else if(g.type==="LineString")line(g.coordinates);else if(g.type==="Polygon"||g.type==="MultiLineString")g.coordinates.forEach(line);else if(g.type==="MultiPolygon")g.coordinates.forEach(p=>p.forEach(line));
 const v=build(vertices),s=build(segments);
 return (target:XY,radius:number)=>{const box:Box=[target[0]-radius,target[1]-radius,target[0]+radius,target[1]+radius];return {vertices:query(v,box),segments:query(s,box)}};
}
const cache=new WeakMap<object,{signature:string;indexes:WeakMap<Geometry,ReturnType<typeof createSnapIndex>>;mercator:WeakMap<Geometry,ReturnType<typeof createSnapIndex>>}>();
export function indexedSnapCandidates(map:object,signature:string,g:Geometry,project:(p:Position)=>XY|null,mercator:boolean,target:XY,radius:number){
 let c=cache.get(map);if(!c){c={signature,indexes:new WeakMap(),mercator:new WeakMap()};cache.set(map,c)}else if(c.signature!==signature){c.signature=signature;c.indexes=new WeakMap()}
 const indexes=mercator?c.mercator:c.indexes;let index=indexes.get(g);if(!index){index=createSnapIndex(g,project,mercator);indexes.set(g,index)}
 return index(target,radius);
}
