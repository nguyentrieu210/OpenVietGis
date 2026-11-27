export type LayoutRect={id:string;x:number;y:number;w:number;h:number};
export type LayoutHistory<T>={past:T[];future:T[]};
export function rememberLayout<T>(h:LayoutHistory<T>,previous:T,next:T):LayoutHistory<T>{
 return JSON.stringify(previous)===JSON.stringify(next)?h:{past:[...h.past,previous].slice(-80),future:[]};
}
export function undoLayout<T>(h:LayoutHistory<T>,current:T){
 return h.past.length?{document:h.past.at(-1)!,history:{past:h.past.slice(0,-1),future:[current,...h.future].slice(0,80)}}:null;
}
export function redoLayout<T>(h:LayoutHistory<T>,current:T){
 return h.future.length?{document:h.future[0]!,history:{past:[...h.past,current].slice(-80),future:h.future.slice(1)}}:null;
}
export type Align="left"|"right"|"top"|"bottom"|"center-x"|"center-y"|"distribute-x"|"distribute-y";
export function alignLayout<T extends LayoutRect>(items:T[],selection:ReadonlySet<string>,mode:Align,pageWidth:number,pageHeight:number):T[]{
 const chosen=items.filter(i=>selection.has(i.id));if(!chosen.length)return items;
 const left=chosen.length===1?0:Math.min(...chosen.map(i=>i.x)),right=chosen.length===1?pageWidth:Math.max(...chosen.map(i=>i.x+i.w)),top=chosen.length===1?0:Math.min(...chosen.map(i=>i.y)),bottom=chosen.length===1?pageHeight:Math.max(...chosen.map(i=>i.y+i.h));
 const result=new Map<string,Partial<LayoutRect>>();
 if(mode==="distribute-x"||mode==="distribute-y"){
  if(chosen.length<3)return items;
  const horizontal=mode==="distribute-x",pos=horizontal?"x":"y",len=horizontal?"w":"h",sorted=[...chosen].sort((a,b)=>a[pos]-b[pos]||a.id.localeCompare(b.id));
  const start=sorted[0]![pos],end=sorted.at(-1)![pos]+sorted.at(-1)![len],gap=(end-start-sorted.reduce((sum,i)=>sum+i[len],0))/(sorted.length-1);
  if(gap<0)throw Error("Không đủ khoảng trống để phân bố không chồng lấn.");
  let at=start;for(const i of sorted){result.set(i.id,{[pos]:at});at+=i[len]+gap}
 }else for(const i of chosen){
  const x=mode==="left"?left:mode==="right"?right-i.w:mode==="center-x"?(left+right-i.w)/2:i.x;
  const y=mode==="top"?top:mode==="bottom"?bottom-i.h:mode==="center-y"?(top+bottom-i.h)/2:i.y;
  result.set(i.id,{x,y});
 }
 return items.map(i=>{const d=result.get(i.id);return d?{...i,x:Math.max(0,Math.min(pageWidth-i.w,d.x??i.x)),y:Math.max(0,Math.min(pageHeight-i.h,d.y??i.y))}:i});
}
