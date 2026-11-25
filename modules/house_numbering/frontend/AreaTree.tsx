import React,{useMemo,useState} from "react";
import {emptySurveyFilter,needsWork,surveyText,type ScopeSignoff,type SurveyFilter,type SurveyRow} from "./SurveyNavigator";

// Spec 051 FR-063: parent → child tree Xã → Ấp → Đường → Hẻm built from the data (areas catalogue + survey points).
export type Tally={n:number;pending:number};
export type SurveyTree=Map<string,Tally&{roads:Map<string,Tally&{alleys:Map<string,Tally>}>}>;
const add=(t:Tally,pending:boolean)=>{t.n++;if(pending)t.pending++};
export function surveyTree(rows:readonly SurveyRow[]):SurveyTree{
 const tree:SurveyTree=new Map();
 for(const r of rows){
  const ap=surveyText(r,"ap");if(!ap)continue;
  const road=surveyText(r,"ten_duong")||"(Chưa có tên đường)",hem=surveyText(r,"hem"),p=needsWork(r);
  let a=tree.get(ap);if(!a)tree.set(ap,a={n:0,pending:0,roads:new Map()});add(a,p);
  let d=a.roads.get(road);if(!d)a.roads.set(road,d={n:0,pending:0,alleys:new Map()});add(d,p);
  if(hem){let h=d.alleys.get(hem);if(!h)d.alleys.set(hem,h={n:0,pending:0});add(h,p)}
 }
 return tree;
}
const natural=(a:string,b:string)=>a.localeCompare(b,"vi",{numeric:true});
const pct=(t:Tally)=>t.n?Math.floor((t.n-t.pending)*100/t.n):0;
type Area={id:string;name:string;parent_id:string|null};
type Props={areas:readonly Area[];rows:readonly SurveyRow[];filter:SurveyFilter;change:(f:SurveyFilter)=>void;disabled:boolean;signedFor?:((ap:string,road:string)=>ScopeSignoff|undefined)|undefined};
export function AreaTree({areas,rows,filter,change,disabled,signedFor}:Props){
 const tree=useMemo(()=>surveyTree(rows),[rows]);
 // Communes start open and the path of the current filter opens itself; the user can still collapse any node.
 const [open,setOpen]=useState<ReadonlySet<string>>(()=>new Set(areas.filter(a=>!a.parent_id).map(a=>"x:"+a.id)));
 const [closed,setClosed]=useState<ReadonlySet<string>>(new Set());
 const shown=(k:string)=>(open.has(k)||k==="a:"+filter.area||k==="r:"+filter.area+"|"+filter.road)&&!closed.has(k);
 const flip=(set:ReadonlySet<string>,k:string,on:boolean)=>{const n=new Set(set);if(on)n.add(k);else n.delete(k);return n};
 const toggle=(k:string)=>{const now=shown(k);setOpen(s=>flip(s,k,!now));setClosed(s=>flip(s,k,now))};
 // Communes are catalogue roots; hamlets come from the catalogue children plus any survey "ap" not in it.
 const roots=areas.filter(a=>!a.parent_id||!areas.some(p=>p.id===a.parent_id));
 const childNames=(id:string)=>areas.filter(a=>a.parent_id===id).map(a=>a.name);
 const listed=new Set(areas.map(a=>a.name));
 const orphans=[...tree.keys()].filter(ap=>!listed.has(ap)).sort(natural);
 const row=(key:string,depth:number,label:string,t:Tally|undefined,active:boolean,select:()=>void,kids:boolean,signed?:ScopeSignoff)=><div className={"hn-tree-row"+(active?" active":"")} style={{paddingLeft:6+depth*14}} key={key} role="treeitem" aria-level={depth+1} aria-selected={active} aria-expanded={kids?shown(key):undefined}>
  {kids?<button type="button" className="hn-tree-toggle" aria-label={(shown(key)?"Thu gọn ":"Mở ")+label} onClick={()=>toggle(key)}>{shown(key)?"▾":"▸"}</button>:<span className="hn-tree-toggle"/>}
  <button type="button" className="hn-tree-label" disabled={disabled} onClick={select}><span>{label}</span>{t&&<small>{t.n.toLocaleString("vi-VN")} điểm · {pct(t)}%</small>}{signed&&<b title="Đã ký duyệt, đang khóa sửa">✓</b>}</button>
 </div>;
 const hamlet=(ap:string,depth:number)=>{const a=tree.get(ap),k="a:"+ap,roads=a?[...a.roads.keys()].sort(natural):[];return <React.Fragment key={k}>
  {row(k,depth,ap,a,filter.area===ap&&!filter.road,()=>change({...emptySurveyFilter(),area:ap}),roads.length>0,signedFor?.(ap,""))}
  {shown(k)&&roads.map(road=>{const d=a!.roads.get(road)!,rk="r:"+ap+"|"+road,alleys=[...d.alleys.keys()].sort(natural);return <React.Fragment key={rk}>
   {row(rk,depth+1,road,d,filter.area===ap&&filter.road===road&&!filter.alley,()=>change({...emptySurveyFilter(),area:ap,road}),alleys.length>0,signedFor?.(ap,road))}
   {shown(rk)&&alleys.map(hem=>row("h:"+ap+"|"+road+"|"+hem,depth+2,"Hẻm "+hem,d.alleys.get(hem),filter.area===ap&&filter.road===road&&filter.alley===hem,()=>change({...emptySurveyFilter(),area:ap,road,alley:hem}),false))}
  </React.Fragment>})}
 </React.Fragment>};
 return <div className="hn-tree" role="tree" aria-label="Cây địa bàn số nhà">
  {roots.map(x=>tree.has(x.name)?hamlet(x.name,0):(()=>{const k="x:"+x.id,kids=childNames(x.id).filter(n=>tree.has(n)).sort(natural),t=kids.reduce<Tally>((s,n)=>({n:s.n+tree.get(n)!.n,pending:s.pending+tree.get(n)!.pending}),{n:0,pending:0});return <React.Fragment key={k}>
   {row(k,0,x.name,t.n?t:undefined,!filter.area,()=>change(emptySurveyFilter()),kids.length>0)}
   {shown(k)&&kids.map(ap=>hamlet(ap,1))}
  </React.Fragment>})())}
  {orphans.map(ap=>hamlet(ap,0))}
 </div>;
}
