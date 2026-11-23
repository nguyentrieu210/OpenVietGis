export type Point2=readonly [number,number];
export type DrawingUnit="meter"|"centimeter"|"millimeter";
export type ControlPoint=Readonly<{drawing:Point2;projected:Point2}>;
export type GeorefInput=Readonly<{crsCode:number;projectedMetres:true;units:DrawingUnit;controls:readonly ControlPoint[];toleranceMetres:number;maxScaleDeviation:number}>;
export type GeorefPreview=Readonly<{crsCode:number;units:DrawingUnit;scale:number;rotationRadians:number;origin:Point2;translation:Point2;rmseMetres:number;maxResidualMetres:number;residuals:readonly number[]}>;
const scales:Record<DrawingUnit,number>={meter:1,centimeter:.01,millimeter:.001};
const valid=(p:Point2)=>p.length===2&&p.every(n=>Number.isFinite(n)&&Math.abs(n)<=1e9);
export function fitCadGeoreference(i:GeorefInput):GeorefPreview{
 if(i.projectedMetres!==true||!Number.isInteger(i.crsCode)||i.crsCode<=0||i.crsCode===4326)throw Error("Explicit projected metre CRS required");
 const u=scales[i.units];if(!u||!Number.isFinite(i.toleranceMetres)||i.toleranceMetres<=0||!Number.isFinite(i.maxScaleDeviation)||i.maxScaleDeviation<0||i.maxScaleDeviation>.2)throw Error("Invalid units or tolerances");
 const c=i.controls;if(c.length<3||c.length>100||c.some(p=>!valid(p.drawing)||!valid(p.projected)))throw Error("3–100 valid control points required");
 const mean=(which:"drawing"|"projected"):Point2=>[c.reduce((s,p)=>s+p[which][0],0)/c.length,c.reduce((s,p)=>s+p[which][1],0)/c.length];
 const o=mean("drawing"),w=mean("projected");let xx=0,yy=0,xy=0,a=0,b=0;
 for(const p of c){const x=(p.drawing[0]-o[0])*u,y=(p.drawing[1]-o[1])*u,X=p.projected[0]-w[0],Y=p.projected[1]-w[1];xx+=x*x;yy+=y*y;xy+=x*y;a+=x*X+y*Y;b+=x*Y-y*X}
 const norm=xx+yy;
 if(norm<1e-12||xx*yy-xy*xy<=1e-10*norm*norm)throw Error("Control points are collinear or degenerate");
 const scale=Math.hypot(a,b)/norm,rotationRadians=Math.atan2(b,a);
 if(!Number.isFinite(scale)||scale<=0||Math.abs(scale-1)>i.maxScaleDeviation+1e-9)throw Error("Scale inconsistent with declared drawing units");
 const preview={crsCode:i.crsCode,units:i.units,scale,rotationRadians,origin:o,translation:w,rmseMetres:0,maxResidualMetres:0,residuals:[] as number[]};
 const residuals=c.map(p=>{const v=projectCadPoint(preview,p.drawing);return Math.hypot(v[0]-p.projected[0],v[1]-p.projected[1])});
 const maxResidualMetres=Math.max(...residuals),rmseMetres=Math.sqrt(residuals.reduce((s,r)=>s+r*r,0)/c.length);
 if(maxResidualMetres>i.toleranceMetres)throw Error("Control point residual exceeds tolerance");
 return {...preview,residuals,maxResidualMetres,rmseMetres};
}
export function projectCadPoint(t:GeorefPreview,p:Point2):Point2{
 if(!valid(p))throw Error("Invalid CAD point");
 const u=scales[t.units]*t.scale,x=(p[0]-t.origin[0])*u,y=(p[1]-t.origin[1])*u,c=Math.cos(t.rotationRadians),s=Math.sin(t.rotationRadians);
 const result:Point2=[t.translation[0]+c*x-s*y,t.translation[1]+s*x+c*y];
 if(!valid(result))throw Error("Invalid projected point");return result;
}
