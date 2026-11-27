import React,{useState,useMemo} from "react";
import {commandDisabledReason,type CoreCommand} from "./CoreCommandBar";
import {GisIcon} from "./GisIcon";
import {searchCommands} from "./command-search";
import {readCommandIds,recordRecent} from "./workspace-preferences";
import "./processing-toolbox.css";
const groups=[["Phân tích","Xử lý không gian"],["CAD","Hình học / CAD"],["Biên tập","Biên tập dữ liệu"],["Đo đạc","Đo đạc"],["Layer","Quản lý dữ liệu"],["Xuất bản","Nhập, xuất và trình bày"]];
const read=(key:string)=>{try{return readCommandIds(localStorage.getItem(key))}catch{return []}};
const write=(key:string,ids:string[])=>{try{localStorage.setItem(key,JSON.stringify(ids))}catch{/* storage unavailable */}};
export function ProcessingToolbox({commands,close,storageKey}:{commands:CoreCommand[];close:()=>void;storageKey:string}){
 const [query,setQuery]=useState(""),[available,setAvailable]=useState(false),[group,setGroup]=useState(""),[view,setView]=useState<"all"|"favorites"|"recent">("all");
 const [favorites,setFavorites]=useState(()=>read(storageKey+":favorites")),[recent,setRecent]=useState(()=>read(storageKey+":recent"));
 const items=useMemo(()=>{
  const matches=searchCommands(commands.filter(c=>c.alias!=="TOOLBOX"&&groups.some(([g])=>g===c.group)&&(!available||!c.disabled)&&(!group||c.group===group)),query,{},commands.length);
  if(view==="all")return matches;
  const ids=view==="favorites"?favorites:recent;
  return ids.flatMap(id=>matches.filter(c=>c.alias===id));
 },[query,available,group,commands,view,favorites,recent]);
 const toggleFavorite=(id:string)=>{const next=favorites.includes(id)?favorites.filter(x=>x!==id):[...favorites,id].slice(0,50);setFavorites(next);write(storageKey+":favorites",next)};
 const run=(c:CoreCommand)=>{if(c.disabled)return;const next=recordRecent(recent,c.alias);setRecent(next);write(storageKey+":recent",next);close();window.dispatchEvent(new CustomEvent("ovg:menu-command",{detail:c.alias}))};
 const row=(c:CoreCommand)=><div className="processing-command-row" key={c.alias}><button type="button" disabled={c.disabled} title={c.disabled?commandDisabledReason(c):c.hint??c.label} onClick={()=>run(c)}><GisIcon name={c.group==="CAD"?"line":c.group==="Phân tích"?"query":"layer"} size={16}/><span>{c.label}</span><code>{c.alias}</code></button><button type="button" className="processing-star" aria-label={(favorites.includes(c.alias)?"Bỏ yêu thích ":"Yêu thích ")+c.label} aria-pressed={favorites.includes(c.alias)} onClick={()=>toggleFavorite(c.alias)}>{favorites.includes(c.alias)?"★":"☆"}</button></div>;
 return <aside className="panel processing-toolbox" aria-label="Hộp công cụ xử lý">
  <header><div><small>GIS CORE · PROCESSING</small><b>Hộp công cụ xử lý</b></div><button aria-label="Đóng hộp công cụ xử lý" onClick={close}><GisIcon name="close"/></button></header>
  <section><label className="processing-search"><GisIcon name="search" size={16}/><input aria-label="Tìm thuật toán" placeholder="Tìm Buffer, Clip, Topology…" value={query} onChange={e=>setQuery(e.target.value)}/></label>
   <div className="processing-views" role="group" aria-label="Danh sách công cụ">{([["all","Tất cả"],["favorites","Yêu thích"],["recent","Đã mở gần đây"]] as const).map(([id,label])=><button type="button" key={id} aria-pressed={view===id} onClick={()=>setView(id)}>{label}</button>)}</div>
   <div className="processing-filter"><select aria-label="Nhóm công cụ" value={group} onChange={e=>setGroup(e.target.value)}><option value="">Tất cả nhóm</option>{groups.map(([g,label])=><option key={g} value={g}>{label}</option>)}</select><label><input type="checkbox" checked={available} onChange={e=>setAvailable(e.target.checked)}/> Khả dụng</label></div><small role="status">{items.length} công cụ · {items.filter(c=>!c.disabled).length} khả dụng</small>
  </section><div className="processing-list">{view==="all"?groups.map(([g,label])=>{const matched=items.filter(c=>c.group===g);return matched.length?<details key={g} open><summary>{label} <span>{matched.length}</span></summary>{matched.map(row)}</details>:null}):items.map(row)}{!items.length&&<p>Không có công cụ phù hợp.</p>}</div>
 </aside>
}
