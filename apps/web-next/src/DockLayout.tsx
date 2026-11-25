import React,{useEffect,useRef,useState} from "react";
import {createPortal} from "react-dom";

// Spec 057 bước B: QGIS-style dock placement for the main panels, remembered per browser.
export type DockLayout={layers:"left"|"right";inspector:"right"|"left"|"float";table:"bottom"|"right"|"float";browser:boolean;legend:boolean;tabs:boolean};
export const defaultDockLayout:DockLayout={layers:"left",inspector:"right",table:"bottom",browser:true,legend:true,tabs:false};
const KEY="ovg:layout:docks";
export function cleanDockLayout(x:unknown):DockLayout{const v=(x&&typeof x==="object"?x:{}) as Partial<DockLayout>;return {layers:v.layers==="right"?"right":"left",inspector:v.inspector==="left"||v.inspector==="float"?v.inspector:"right",table:v.table==="right"||v.table==="float"?v.table:"bottom",browser:v.browser!==false,legend:v.legend!==false,tabs:v.tabs===true}}
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
  {side("inspector","Khung thông tin",[["right","Phải"],["left","Trái"],["float","Nổi"]])}
  {side("table","Bảng thuộc tính",[["bottom","Dưới"],["right","Phải"],["float","Nổi"]])}
  <label><input type="checkbox" checked={layout.tabs} onChange={e=>set({tabs:e.target.checked})}/> Khung Lớp dạng tab (Lớp · Trình duyệt · Chú giải)</label>
  <button type="button" onClick={()=>{set(null);setOpen(false)}}>Khôi phục bố cục mặc định</button>
 </div>,document.body)}</div>;
}

// Floating panel: drag the title strip to move, drag the corner to resize (CSS resize), ⤓ docks it back.
export function FloatHandle({storageKey,title,dock}:{storageKey:string;title:string;dock:()=>void}){
 const ref=useRef<HTMLDivElement>(null);
 const place=(x:number,y:number)=>{const p=ref.current?.parentElement;if(!p)return;const r=p.getBoundingClientRect();const nx=Math.max(0,Math.min(innerWidth-80,x)),ny=Math.max(0,Math.min(innerHeight-40,y));p.style.setProperty("--ovg-fx",nx+"px");p.style.setProperty("--ovg-fy",ny+"px");return [nx,ny,r.width,r.height]};
 useEffect(()=>{try{const v=JSON.parse(localStorage.getItem(storageKey)??"null");if(Array.isArray(v)&&v.every(Number.isFinite)){place(v[0],v[1]);const p=ref.current?.parentElement;if(p&&v[2]>120&&v[3]>80){p.style.width=Math.min(v[2],innerWidth)+"px";p.style.height=Math.min(v[3],innerHeight)+"px"}}}catch{/* ignore */}},[storageKey]);
 const save=()=>{const p=ref.current?.parentElement;if(!p)return;const r=p.getBoundingClientRect();try{localStorage.setItem(storageKey,JSON.stringify([r.left,r.top,r.width,r.height]))}catch{/* storage unavailable */}};
 useEffect(()=>{const p=ref.current?.parentElement;if(!p)return;const o=new ResizeObserver(()=>save());o.observe(p);return()=>o.disconnect()},[storageKey]);
 const start=(e:React.PointerEvent<HTMLDivElement>)=>{if((e.target as HTMLElement).closest("button"))return;e.preventDefault();const p=ref.current?.parentElement;if(!p)return;const r=p.getBoundingClientRect(),dx=e.clientX-r.left,dy=e.clientY-r.top,el=e.currentTarget;el.setPointerCapture(e.pointerId);
  const move=(ev:PointerEvent)=>place(ev.clientX-dx,ev.clientY-dy);const end=()=>{save();el.removeEventListener("pointermove",move);el.removeEventListener("pointerup",end);el.removeEventListener("pointercancel",end)};
  el.addEventListener("pointermove",move);el.addEventListener("pointerup",end);el.addEventListener("pointercancel",end)};
 return <div ref={ref} className="ovg-float-bar" onPointerDown={start} title="Kéo để di chuyển · kéo góc dưới phải để đổi kích thước"><span aria-hidden="true">⠿</span><b>{title}</b><button type="button" onClick={dock} aria-label={"Gắn lại "+title} title="Gắn lại vào bố cục">⤓</button></div>;
}
