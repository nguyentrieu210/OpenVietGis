import React,{useEffect,useMemo,useState} from "react";
import {listProcessingAlgorithms,type ProcessingAlgorithm} from "@openvietgis/client-sdk";
import {commandDisabledReason,type CoreCommand} from "./CoreCommandBar";
import {GisIcon} from "./GisIcon";
import {searchProcessingToolboxEntries,filterProcessingToolboxEntries,processingToolboxEntries,type ProcessingToolboxEntry} from "./processing-toolbox-registry";
import {readCommandIds,recordRecent} from "./workspace-preferences";
import {apiClient} from "./workbenches/app/api";
import "./processing-toolbox.css";

const groups=[["Phân tích","Xử lý không gian"],["CAD","Hình học / CAD"],["Biên tập","Biên tập dữ liệu"],["Đo đạc","Đo đạc"],["Layer","Quản lý dữ liệu"],["Xuất bản","Nhập, xuất và trình bày"]];
const read=(key:string)=>{try{return readCommandIds(localStorage.getItem(key))}catch{return []}};
const write=(key:string,ids:string[])=>{try{localStorage.setItem(key,JSON.stringify(ids))}catch{/* storage unavailable */}};

export function ProcessingToolbox({commands,close,storageKey,organizationId,projectId}:{commands:CoreCommand[];close:()=>void;storageKey:string;organizationId:string;projectId:string}){
 const [query,setQuery]=useState(""),[available,setAvailable]=useState(false),[provider,setProvider]=useState(""),[category,setCategory]=useState(""),[capability,setCapability]=useState(""),[view,setView]=useState<"all"|"favorites"|"recent">("all");
 const [favorites,setFavorites]=useState(()=>read(storageKey+":favorites")),[recent,setRecent]=useState(()=>read(storageKey+":recent"));
 const [algorithms,setAlgorithms]=useState<ProcessingAlgorithm[]|null>(null),[catalogError,setCatalogError]=useState(""),[reload,setReload]=useState(0);
 useEffect(()=>{
  let active=true;
  setAlgorithms(null);setCatalogError("");
  if(!organizationId||!projectId){setCatalogError("Không xác định được tổ chức hoặc dự án.");return()=>{active=false}}
  void listProcessingAlgorithms(apiClient,organizationId,projectId).then(result=>{if(active)setAlgorithms(result.items)}).catch(()=>{if(active){setAlgorithms([]);setCatalogError("Không tải được danh mục xử lý.")}});
  return()=>{active=false};
 },[organizationId,projectId,reload]);
 const registered=useMemo(()=>algorithms?processingToolboxEntries(commands,algorithms):[],[commands,algorithms]);
 const providers=useMemo(()=>{const names=new Map<string,string>();for(const {algorithm} of registered)names.set(algorithm.provider_id,algorithm.provider_name);return [...names.entries()]},[registered]);
 const categories=useMemo(()=>[...new Set(registered.map(x=>x.algorithm.category))].sort(),[registered]);
 const capabilities=useMemo(()=>[...new Set(registered.flatMap(x=>x.algorithm.capabilities))].sort(),[registered]);
 const items=useMemo(()=>{
  const filtered=filterProcessingToolboxEntries(registered,{providerId:provider,category,capability,availableOnly:available});
  const matches=searchProcessingToolboxEntries(filtered,query);
  if(view==="all")return matches;
  const ids=view==="favorites"?favorites:recent;
  return ids.flatMap(id=>matches.filter(x=>x.command.alias===id));
 },[query,available,provider,category,capability,registered,view,favorites,recent]);
 const toggleFavorite=(id:string)=>{const next=favorites.includes(id)?favorites.filter(x=>x!==id):[...favorites,id].slice(0,50);setFavorites(next);write(storageKey+":favorites",next)};
 const run=(c:CoreCommand)=>{if(c.disabled)return;const next=recordRecent(recent,c.alias);setRecent(next);write(storageKey+":recent",next);close();window.dispatchEvent(new CustomEvent("ovg:menu-command",{detail:c.alias}))};
 const row=(entry:ProcessingToolboxEntry)=>{const c=entry.command,a=entry.algorithm;const metadata=a.provider_name+" · "+a.category+" · "+a.capabilities.join(", ");const title=(c.disabled?commandDisabledReason(c):a.description)+" · "+metadata;return <div className="processing-command-row" key={c.alias}><button type="button" disabled={c.disabled} title={title} onClick={()=>run(c)}><GisIcon name={c.group==="CAD"?"line":c.group==="Phân tích"?"query":"layer"} size={16}/><span><b>{c.label}</b><small className="processing-algorithm-meta">{metadata}</small></span><code>{c.alias}</code></button><button type="button" className="processing-star" aria-label={(favorites.includes(c.alias)?"Bỏ yêu thích ":"Yêu thích ")+c.label} aria-pressed={favorites.includes(c.alias)} onClick={()=>toggleFavorite(c.alias)}>{favorites.includes(c.alias)?"★":"☆"}</button></div>};
 return <aside className="panel processing-toolbox" aria-label="Hộp công cụ xử lý">
  <header><div><small>GIS CORE · PROCESSING</small><b>Hộp công cụ xử lý</b></div><button aria-label="Đóng hộp công cụ xử lý" onClick={close}><GisIcon name="close"/></button></header>
  <section>
   <label className="processing-search"><GisIcon name="search" size={16}/><input aria-label="Tìm thuật toán" placeholder="Tìm Buffer, Clip, Dissolve…" value={query} onChange={e=>setQuery(e.target.value)}/></label>
   <div className="processing-views" role="group" aria-label="Danh sách công cụ">{([["all","Tất cả"],["favorites","Yêu thích"],["recent","Đã mở gần đây"]] as const).map(([id,label])=><button type="button" key={id} aria-pressed={view===id} onClick={()=>setView(id)}>{label}</button>)}</div>
   <div className="processing-filter processing-filter-registry"><select aria-label="Danh mục thuật toán" value={category} onChange={e=>setCategory(e.target.value)}><option value="">Tất cả danh mục</option>{categories.map(value=><option key={value} value={value}>{value}</option>)}</select><select aria-label="Nhà cung cấp xử lý" value={provider} onChange={e=>setProvider(e.target.value)}><option value="">Tất cả nhà cung cấp</option>{providers.map(([id,name])=><option key={id} value={id}>{name}</option>)}</select></div>
   <div className="processing-filter processing-filter-registry"><select aria-label="Khả năng xử lý" value={capability} onChange={e=>setCapability(e.target.value)}><option value="">Tất cả khả năng</option>{capabilities.map(value=><option key={value} value={value}>{value}</option>)}</select><label><input type="checkbox" checked={available} onChange={e=>setAvailable(e.target.checked)}/> Khả dụng</label></div>
   {catalogError?<small role="alert">{catalogError}</small>:algorithms===null?<small role="status">Đang tải danh mục thuật toán…</small>:<small role="status">{items.length} thuật toán · {items.filter(x=>!x.command.disabled).length} khả dụng</small>}
  </section>
  <div className="processing-list">
   {catalogError?<div className="processing-catalog-error" role="alert"><span>{catalogError}</span><button type="button" onClick={()=>setReload(x=>x+1)}>Thử lại</button></div>:algorithms===null?<p role="status">Đang tải danh mục xử lý…</p>:!algorithms.length?<p>Máy chủ chưa đăng ký thuật toán xử lý.</p>:items.length?(view==="all"?groups.map(([g,label])=>{const matched=items.filter(x=>x.command.group===g);return matched.length?<details key={g} open><summary>{label} <span>{matched.length}</span></summary>{matched.map(row)}</details>:null}):items.map(row)):<p>Không có thuật toán phù hợp.</p>}
  </div>
 </aside>
}
