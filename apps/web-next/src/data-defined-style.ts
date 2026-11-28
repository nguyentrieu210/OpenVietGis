/** Typed field-based paint overrides, intentionally no eval or arbitrary expression execution. */
type Field={field_id:string;type:string};
export type DataDefinedStyle={color:string;opacity:string;width:string;radius:string};
export const emptyDataDefined:DataDefinedStyle={color:"",opacity:"",width:"",radius:""};
export function cleanDataDefined(raw:unknown):DataDefinedStyle{
 const v=(raw&&typeof raw==="object"?raw:{}) as Partial<DataDefinedStyle>;
 const clean=(x:unknown)=>typeof x==="string"&&x.length<=128?x:"";
 return {color:clean(v.color),opacity:clean(v.opacity),width:clean(v.width),radius:clean(v.radius)};
}
export function hasDataDefined(raw:unknown){return Object.values(cleanDataDefined(raw)).some(Boolean)}
const number=(v:unknown)=>typeof v==="number"?v:typeof v==="string"&&/^[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][-+]?\d+)?$/.test(v)?Number(v):NaN;
export function dataDefinedPaint(raw:unknown,attrs:Record<string,unknown>,fields:readonly Field[],layerOpacity=1){
 const v=cleanDataDefined(raw),out:Partial<{color:string;opacity:number;width:number;strokeWidth:number;radius:number}>={};
 const value=(key:string,types:string[])=>key&&fields.some(f=>f.field_id===key&&types.includes(f.type))&&Object.hasOwn(attrs,key)?attrs[key]:undefined;
 const color=value(v.color,["text"]);if(typeof color==="string"&&/^#[a-f0-9]{6}$/i.test(color))out.color=color;
 const bounded=(id:string,min:number,max:number)=>{const n=number(value(id,["decimal","integer"]));return Number.isFinite(n)&&n>=min&&n<=max?n:undefined};
 const opacity=bounded(v.opacity,0,1),width=bounded(v.width,1,12),radius=bounded(v.radius,2,20);
 if(opacity!==undefined)out.opacity=opacity*Math.max(0,Math.min(1,layerOpacity));
 if(width!==undefined){out.width=width;out.strokeWidth=width}if(radius!==undefined)out.radius=radius;
 return out;
}
