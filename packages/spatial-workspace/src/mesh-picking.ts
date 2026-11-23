/** Bounded triangle hit-test in the same clip-space projection as the GPU.
 * Depth is NDC z: smaller is closer, independent of model origin. */
export function meshDepth(vertices:Float32Array,m:ArrayLike<number>,x:number,y:number):number|null{
 if(!Number.isFinite(x)||!Number.isFinite(y)||Math.abs(x)>1||Math.abs(y)>1||m.length!==16)return null;
 let best=Infinity;
 const a=new Float64Array(9);
 for(let i=0;i+8<vertices.length;i+=9){
  let valid=true;
  for(let j=0;j<3;j++){
   const q=i+j*3,v=vertices[q]!,u=vertices[q+1]!,w=vertices[q+2]!;
   const cw=m[3]!*v+m[7]!*u+m[11]!*w+m[15]!;
   if(!Number.isFinite(cw)||cw<=1e-9){valid=false;break}
   a[j*3]=(m[0]!*v+m[4]!*u+m[8]!*w+m[12]!)/cw;
   a[j*3+1]=(m[1]!*v+m[5]!*u+m[9]!*w+m[13]!)/cw;
   a[j*3+2]=(m[2]!*v+m[6]!*u+m[10]!*w+m[14]!)/cw;
  }
  if(!valid)continue;
  const ax=a[0]!,ay=a[1]!,bx=a[3]!,by=a[4]!,cx=a[6]!,cy=a[7]!;
  if(x<Math.min(ax,bx,cx)||x>Math.max(ax,bx,cx)||y<Math.min(ay,by,cy)||y>Math.max(ay,by,cy))continue;
  const den=(by-cy)*(ax-cx)+(cx-bx)*(ay-cy);
  if(Math.abs(den)<1e-15)continue;
  const s=((by-cy)*(x-cx)+(cx-bx)*(y-cy))/den,t=((cy-ay)*(x-cx)+(ax-cx)*(y-cy))/den,r=1-s-t;
  if(s< -1e-7||t< -1e-7||r< -1e-7)continue;
  const z=s*a[2]!+t*a[5]!+r*a[8]!;
  if(z>=-1&&z<=1&&z<best)best=z;
 }
 return Number.isFinite(best)?best:null;
}
export const MAX_SCENE_MODELS=8;
export const MAX_SCENE_TRIANGLES=250000;
export function checkSceneBudget(models:readonly {triangles:number}[]):void{
 if(models.length>MAX_SCENE_MODELS||models.some(x=>!Number.isInteger(x.triangles)||x.triangles<1||x.triangles>100000)||models.reduce((sum,x)=>sum+x.triangles,0)>MAX_SCENE_TRIANGLES)throw Error("Cảnh tối đa 8 mô hình, tổng 250.000 tam giác.");
}
