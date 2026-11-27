import type {StudioRow} from "./studio-contract";
type Ramp={gradFrom:string;gradTo:string;gradBreaks:number[];gradField:string};
// Spec 056 đợt 2: graduated ("theo khoảng") symbology like QGIS — equal interval or quantile classes, colour ramp from→to.
export const numeric=(x:unknown)=>{if(typeof x!=="number"&&(typeof x!=="string"||!/^[-+]?\d+(?:[.,]\d+)?$/.test(x.trim())))return null;const n=typeof x==="number"?x:Number(x.trim().replace(",","."));return Number.isFinite(n)?n:null};
export function computeBreaks(values:readonly number[],classes:number,mode:"equal"|"quantile"):number[]{const v=values.filter(Number.isFinite).slice().sort((a,b)=>a-b);if(!v.length)return [];const lo=v[0]!,hi=v[v.length-1]!;if(lo===hi)return [lo,hi];const k=Number.isFinite(classes)?Math.max(2,Math.min(9,Math.round(classes))):5;const edges=mode==="equal"?Array.from({length:k+1},(_,i)=>lo+(hi-lo)*i/k):Array.from({length:k+1},(_,i)=>v[Math.min(v.length-1,Math.round((v.length-1)*i/k))]!);return edges.filter((x,i)=>i===0||x>edges[i-1]!)}
const mix=(a:string,b:string,t:number)=>"#"+[1,3,5].map(i=>Math.round(parseInt(a.slice(i,i+2),16)*(1-t)+parseInt(b.slice(i,i+2),16)*t).toString(16).padStart(2,"0")).join("");
export const NO_VALUE_COLOR="#bdbdbd";
// Values outside the breaks (rows loaded or edited after Apply) clamp to the first/last class; only non-numbers get -1.
export function gradClassIndex(x:unknown,breaks:readonly number[]){const n=numeric(x);if(n===null||!validBreaks(breaks))return -1;for(let i=1;i<breaks.length;i++)if(n<=breaks[i]!)return i-1;return breaks.length-2}
export function gradColor(i:number,view:Ramp){const k=view.gradBreaks.length-1;return i<0||k<1?NO_VALUE_COLOR:mix(view.gradFrom,view.gradTo,k===1?1:i/(k-1))}
// Use round-trip numeric precision: two nearby breaks must not share the same legend key.
const fmt=(x:number)=>String(x).replace(".",",");
export function gradLegend(rows:StudioRow[],view:Ramp){const b=view.gradBreaks,counts=new Array(Math.max(0,b.length-1)).fill(0);let none=0;for(const r of rows){const i=gradClassIndex(r.feature.attributes[view.gradField],b);if(i<0)none++;else counts[i]++}const out=counts.map((count,i)=>({key:fmt(b[i]!)+" – "+fmt(b[i+1]!),count,color:gradColor(i,view)}));return none?[...out,{key:"(Không phải số)",count:none,color:NO_VALUE_COLOR}]:out}
export const gradKey=(x:unknown,view:Ramp)=>{const b=view.gradBreaks,i=gradClassIndex(x,b);return i<0?"(Không phải số)":fmt(b[i]!)+" – "+fmt(b[i+1]!)};
export const zoomScale=(z:number)=>"1:"+Math.round(591657550/2**z).toLocaleString("vi-VN");

export function validBreaks(b:unknown):b is number[]{
 return Array.isArray(b)&&b.length>=2&&b.length<=10&&b.every(x=>typeof x==="number"&&Number.isFinite(x))&&(b.length===2&&b[0]===b[1]||b.every((x,i)=>i===0||x>b[i-1]));
}
