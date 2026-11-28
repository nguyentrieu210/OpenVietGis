import type {Geometry,Position} from "geojson";
type XY=[number,number];
type Cell={x:number;y:number;hx:number;hy:number;d:number;max:number};
type Candidate={point:XY;distance:number};
const cache=new WeakMap<Geometry,XY|null>();
// Signed distance to all boundaries: even/odd inclusion handles interior holes.
function distance(x:number,y:number,rings:Position[][]):number{
 let inside=false,min=Infinity;
 for(const ring of rings)for(let i=0,j=ring.length-1;i<ring.length;j=i++){
  const a=ring[j],b=ring[i],dx=b[0]-a[0],dy=b[1]-a[1];
  if((a[1]>y)!==(b[1]>y)&&x<(b[0]-a[0])*(y-a[1])/(b[1]-a[1])+a[0])inside=!inside;
  const len=dx*dx+dy*dy,t=len?Math.max(0,Math.min(1,((x-a[0])*dx+(y-a[1])*dy)/len)):0;
  min=Math.min(min,(x-a[0]-t*dx)**2+(y-a[1]-t*dy)**2);
 }
 return (inside?1:-1)*Math.sqrt(min);
}
function pole(rings:Position[][]):Candidate|null{
 const outer=rings[0];if(!outer||outer.length<4)return null;
 let minX=Infinity,minY=Infinity,maxX=-Infinity,maxY=-Infinity;
 for(const p of outer){if(!Number.isFinite(p[0])||!Number.isFinite(p[1]))return null;minX=Math.min(minX,p[0]);minY=Math.min(minY,p[1]);maxX=Math.max(maxX,p[0]);maxY=Math.max(maxY,p[1])}
 const w=maxX-minX,h=maxY-minY;if(!(w>0&&h>0))return null;
 const precision=Math.max(Math.min(w,h)/100,1e-8);
 const cell=(x:number,y:number,hx:number,hy:number):Cell=>{const d=distance(x,y,rings);return {x,y,hx,hy,d,max:d+Math.hypot(hx,hy)}};
 const heap:Cell[]=[];
 const push=(c:Cell)=>{let i=heap.length;heap.push(c);while(i>0){const p=(i-1)>>1;if(heap[p].max>=c.max)break;heap[i]=heap[p];i=p;heap[i]=c}};
 const pop=()=>{const first=heap[0],last=heap.pop()!;if(heap.length){let i=0;while(i*2+1<heap.length){let j=i*2+1;if(j+1<heap.length&&heap[j+1].max>heap[j].max)j++;if(heap[j].max<=last.max)break;heap[i]=heap[j];i=j}heap[i]=last}return first};
 let best=cell((minX+maxX)/2,(minY+maxY)/2,w/2,h/2);
 // Seed a guaranteed interior candidate for typical concave polygons and holes.
 for(const f of [.5,.25,.75,.125,.375,.625,.875]){
  const y=minY+h*f, crossings:number[]=[];
  for(const ring of rings)for(let i=0,j=ring.length-1;i<ring.length;j=i++){const a=ring[j],b=ring[i];if((a[1]>y)!==(b[1]>y))crossings.push(a[0]+(y-a[1])*(b[0]-a[0])/(b[1]-a[1]))}
  crossings.sort((a,b)=>a-b);
  for(let i=0;i+1<crossings.length;i+=2){const c=cell((crossings[i]+crossings[i+1])/2,y,0,0);if(c.d>best.d)best=c}
 }
 push(cell((minX+maxX)/2,(minY+maxY)/2,w/2,h/2));
 // Bounded best-first polylabel search; split the longest axis to handle narrow parcels.
 const vertices=rings.reduce((n,r)=>n+r.length,0),limit=Math.max(32,Math.min(2048,Math.floor(2000000/Math.max(vertices,1))));
 for(let n=0;heap.length&&n<limit;n++){
  const c=pop();if(c.d>best.d)best=c;if(c.max-best.d<=precision)continue;
  if(c.hx>=c.hy){const d=c.hx/2;if(d<=0)continue;push(cell(c.x-d,c.y,d,c.hy));push(cell(c.x+d,c.y,d,c.hy))}
  else{const d=c.hy/2;if(d<=0)continue;push(cell(c.x,c.y-d,c.hx,d));push(cell(c.x,c.y+d,c.hx,d))}
 }
 return best.d>0?{point:[best.x,best.y],distance:best.d}:null;
}
export function polygonLabelAnchor(g:Extract<Geometry,{type:"Polygon"|"MultiPolygon"}>):XY|null{
 if(cache.has(g))return cache.get(g)!;
 let best:Candidate|null=null;
 for(const polygon of g.type==="Polygon"?[g.coordinates]:g.coordinates){const next=pole(polygon);if(next&&(!best||next.distance>best.distance))best=next}
 const result=best?.point??null;cache.set(g,result);return result;
}
