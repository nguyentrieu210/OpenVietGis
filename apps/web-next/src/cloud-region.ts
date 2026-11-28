/** Quantized camera window, not exact camera-frustum visibility. Keep the root overview. */
export type CloudRegion=readonly [number,number,number,number];
const R=6378137;
export function cameraRegion(lon:number,lat:number,zoom:number):CloudRegion|undefined{
 if(![lon,lat,zoom].every(Number.isFinite))return undefined;
 if(Math.abs(lat)>85.051129||Math.abs(lon)>180)return undefined;
 if(zoom<=4)return undefined;
 const step=360/2**Math.min(20,Math.max(1,Math.floor(zoom)-2)),x=Math.floor((lon+180)/step)*step-180,y=Math.floor((lat+90)/step)*step-90;
 if(x-step< -180||x+2*step>180)return undefined;
 return [x-step,Math.max(-85.05112878,y-step),x+2*step,Math.min(85.05112878,y+2*step)];
}
export function nativeRegion(r:CloudRegion|undefined,crs:4326|3857):CloudRegion|undefined{
 if(!r)return undefined;
 if(r.length!==4||!r.every(Number.isFinite)||r[0]< -180||r[2]>180||r[1]< -85.051129||r[3]>85.051129||r[0]>r[2]||r[1]>r[3])throw Error("Invalid point cloud region");
 if(crs===4326)return r;
 if(crs!==3857)throw Error("Unsupported cloud CRS");
 const x=(n:number)=>R*n*Math.PI/180,y=(n:number)=>R*Math.asinh(Math.tan(n*Math.PI/180));
 return [x(r[0]),y(r[1]),x(r[2]),y(r[3])];
}
export function intersectsCloudNode(key:readonly number[],b:readonly number[],r:CloudRegion|undefined):boolean{
 if(!r||key[0]===0)return true;
 if(b.length!==6||!b.every(Number.isFinite)||b.slice(0,3).some((v,i)=>v>=b[i+3]!))throw Error("Invalid octree extent");
 const [d,x,y]=key as readonly [number,number,number,number],n=2**d;
 const w=b[0]!+(b[3]!-b[0]!)*x/n,e=b[0]!+(b[3]!-b[0]!)*(x+1)/n,s=b[1]!+(b[4]!-b[1]!)*y/n,t=b[1]!+(b[4]!-b[1]!)*(y+1)/n;
 return w<=r[2]&&e>=r[0]&&s<=r[3]&&t>=r[1];
}
