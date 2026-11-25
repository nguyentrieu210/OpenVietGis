import type {StudioRow} from "./studio-contract";
type Ramp={gradFrom:string;gradTo:string;gradBreaks:number[];gradField:string};
// Spec 056 đợt 2: graduated ("theo khoảng") symbology like QGIS — equal interval or quantile classes, colour ramp from→to.
export const numeric=(x:unknown)=>{if(x===null||x===undefined||x==="")return null;const n=typeof x==="number"?x:Number(String(x).replace(",","."));return Number.isFinite(n)?n:null};
export function computeBreaks(values:readonly number[],classes:number,mode:"equal"|"quantile"):number[]{const v=values.filter(Number.isFinite).slice().sort((a,b)=>a-b);if(!v.length)return [];const lo=v[0]!,hi=v[v.length-1]!;if(lo===hi)return [lo,hi];const k=Math.max(2,Math.min(9,Math.round(classes)));const edges=mode==="equal"?Array.from({length:k+1},(_,i)=>lo+(hi-lo)*i/k):Array.from({length:k+1},(_,i)=>v[Math.min(v.length-1,Math.round((v.length-1)*i/k))]!);return edges.filter((x,i)=>i===0||x>edges[i-1]!)}
const mix=(a:string,b:string,t:number)=>"#"+[1,3,5].map(i=>Math.round(parseInt(a.slice(i,i+2),16)*(1-t)+parseInt(b.slice(i,i+2),16)*t).toString(16).padStart(2,"0")).join("");
export const NO_VALUE_COLOR="#bdbdbd";
// Values outside the breaks (rows loaded or edited after Apply) clamp to the first/last class; only non-numbers get -1.
export function gradClassIndex(x:unknown,breaks:readonly number[]){const n=numeric(x);if(n===null||breaks.length<2)return -1;for(let i=1;i<breaks.length;i++)if(n<=breaks[i]!)return i-1;return breaks.length-2}
export function gradColor(i:number,view:Ramp){const k=view.gradBreaks.length-1;return i<0||k<1?NO_VALUE_COLOR:mix(view.gradFrom,view.gradTo,k===1?1:i/(k-1))}
const fmt=(x:number)=>x.toLocaleString("vi-VN",{maximumFractionDigits:2});
export function gradLegend(rows:StudioRow[],view:Ramp){const b=view.gradBreaks,counts=new Array(Math.max(0,b.length-1)).fill(0);let none=0;for(const r of rows){const i=gradClassIndex(r.feature.attributes[view.gradField],b);if(i<0)none++;else counts[i]++}const out=counts.map((count,i)=>({key:fmt(b[i]!)+" – "+fmt(b[i+1]!),count,color:gradColor(i,view)}));return none?[...out,{key:"(Không phải số)",count:none,color:NO_VALUE_COLOR}]:out}
export const zoomScale=(z:number)=>"1:"+Math.round(591657550/2**z).toLocaleString("vi-VN");
