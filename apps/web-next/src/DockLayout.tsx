import React,{useEffect,useRef,useState} from "react";

// Spec 057 bước B: QGIS-style dock placement for the main panels, remembered per browser.
export type DockLayout={layers:"left"|"right";inspector:"right"|"left";table:"bottom"|"right";browser:boolean;legend:boolean};
export const defaultDockLayout:DockLayout={layers:"left",inspector:"right",table:"bottom",browser:true,legend:true};
const KEY="ovg:layout:docks";
export function cleanDockLayout(x:unknown):DockLayout{const v=(x&&typeof x==="object"?x:{}) as Partial<DockLayout>;return {layers:v.layers==="right"?"right":"left",inspector:v.inspector==="left"?"left":"right",table:v.table==="right"?"right":"bottom",browser:v.browser!==false,legend:v.legend!==false}}
export function useDockLayout():[DockLayout,(patch:Partial<DockLayout>|null)=>void]{
 const [layout,setLayout]=useState<DockLayout>(()=>{try{return cleanDockLayout(JSON.parse(localStorage.getItem(KEY)??"null"))}catch{return defaultDockLayout}});
 const update=(patch:Partial<DockLayout>|null)=>setLayout(prev=>{const next=patch?{...prev,...patch}:defaultDockLayout;try{localStorage.setItem(KEY,JSON.stringify(next))}catch{/* storage unavailable */}requestAnimationFrame(()=>window.dispatchEvent(new Event("resize")));return next});
 return [layout,update];
}
type Panels={layers:boolean;table:boolean};
export function DockLayoutMenu({layout,set,panels,toggleLayers,toggleTable}:{layout:DockLayout;set:(patch:Partial<DockLayout>|null)=>void;panels:Panels;toggleLayers:()=>void;toggleTable:()=>void}){
 const [open,setOpen]=useState(false),ref=useRef<HTMLDivElement>(null);
 useEffect(()=>{if(!open)return;const click=(e:PointerEvent)=>{if(!ref.current?.contains(e.target as Node))setOpen(false)};const key=(e:KeyboardEvent)=>{if(e.key==="Escape")setOpen(false)};document.addEventListener("pointerdown",click);document.addEventListener("keydown",key);return()=>{document.removeEventListener("pointerdown",click);document.removeEventListener("keydown",key)}},[open]);
 const side=<K extends "layers"|"inspector"|"table">(k:K,label:string,options:[DockLayout[K],string][])=><fieldset className="dock-menu-row"><legend>{label}</legend>{options.map(([v,t])=><label key={String(v)}><input type="radio" name={"dock-"+k} checked={layout[k]===v} onChange={()=>set({[k]:v} as Partial<DockLayout>)}/>{t}</label>)}</fieldset>;
 return <div className="dock-menu" ref={ref}><button type="button" className="plugin-tool" aria-expanded={open} aria-haspopup="dialog" onClick={()=>setOpen(v=>!v)} title="Bố cục khung: vị trí, ẩn/hiện">▦ Bố cục</button>{open&&<div className="dock-menu-pop" role="dialog" aria-label="Bố cục khung">
  <b>Khung hiển thị</b>
  <label><input type="checkbox" checked={panels.layers} onChange={toggleLayers}/> Lớp dữ liệu</label>
  <label><input type="checkbox" checked={layout.browser} onChange={e=>set({browser:e.target.checked})}/> Trình duyệt</label>
  <label><input type="checkbox" checked={layout.legend} onChange={e=>set({legend:e.target.checked})}/> Chú giải nghiệp vụ</label>
  <label><input type="checkbox" checked={panels.table} onChange={toggleTable}/> Bảng thuộc tính</label>
  <b>Vị trí gắn</b>
  {side("layers","Khung Lớp",[["left","Trái"],["right","Phải"]])}
  {side("inspector","Khung thông tin",[["right","Phải"],["left","Trái"]])}
  {side("table","Bảng thuộc tính",[["bottom","Dưới"],["right","Phải"]])}
  <button type="button" onClick={()=>{set(null);setOpen(false)}}>Khôi phục bố cục mặc định</button>
 </div>}</div>;
}
