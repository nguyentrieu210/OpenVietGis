import React,{useEffect,useRef,useState} from "react";
import {createPortal} from "react-dom";

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
 const [open,setOpen]=useState(false),ref=useRef<HTMLDivElement>(null),pop=useRef<HTMLDivElement>(null),[at,setAt]=useState({left:0,top:0});
 // Portal to <body>: the toolbar sits under the layer dock and map overlays, which covered the menu.
 const toggle=()=>{const r=ref.current?.getBoundingClientRect();if(r)setAt({left:Math.max(4,Math.min(r.left,innerWidth-250)),top:r.bottom+4});setOpen(v=>!v)};
 useEffect(()=>{if(!open)return;const click=(e:PointerEvent)=>{if(!ref.current?.contains(e.target as Node)&&!pop.current?.contains(e.target as Node))setOpen(false)};const key=(e:KeyboardEvent)=>{if(e.key==="Escape")setOpen(false)};const shut=()=>setOpen(false),place=()=>{const r=ref.current?.getBoundingClientRect();if(r)setAt({left:Math.max(4,Math.min(r.left,innerWidth-250)),top:r.bottom+4})};document.addEventListener("pointerdown",click);document.addEventListener("keydown",key);window.addEventListener("resize",place);window.addEventListener("scroll",shut,true);return()=>{document.removeEventListener("pointerdown",click);document.removeEventListener("keydown",key);window.removeEventListener("resize",place);window.removeEventListener("scroll",shut,true)}},[open]);
 const side=<K extends "layers"|"inspector"|"table">(k:K,label:string,options:[DockLayout[K],string][])=><fieldset className="dock-menu-row"><legend>{label}</legend>{options.map(([v,t])=><label key={String(v)}><input type="radio" name={"dock-"+k} checked={layout[k]===v} onChange={()=>set({[k]:v} as Partial<DockLayout>)}/>{t}</label>)}</fieldset>;
 return <div className="dock-menu" ref={ref}><button type="button" className="plugin-tool" aria-expanded={open} aria-haspopup="dialog" onClick={toggle} title="Bố cục khung: vị trí, ẩn/hiện">▦ Bố cục</button>{open&&createPortal(<div ref={pop} className="dock-menu-pop" style={{left:at.left,top:at.top}} role="dialog" aria-label="Bố cục khung">
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
 </div>,document.body)}</div>;
}
