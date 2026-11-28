import React,{useEffect,useRef,useState} from "react";
import {createPortal} from "react-dom";
import {Button,Select} from "./ui/primitives";
import {fitFloatingPanel} from "./workspace-preferences";
import {dockPanelPatch,dockTargetsFor,fitDockMenuPosition,type DockPanelId,type DockTarget} from "./dock-drag";

// Spec 057 bước B: QGIS-style dock placement for the main panels, remembered per browser.
export type DockLayout={layers:"left"|"right";inspector:"right"|"left"|"float";table:"bottom"|"right"|"float";browser:boolean;legend:boolean;tabs:boolean;ribbon:boolean;tools:boolean;icons:"small"|"medium"|"large"};
export const defaultDockLayout:DockLayout={layers:"left",inspector:"right",table:"bottom",browser:true,legend:true,tabs:false,ribbon:true,tools:true,icons:"medium"};
export type DockLayoutPreset="qgis"|"map"|"analysis";
export const dockLayoutPresets:Record<DockLayoutPreset,DockLayout>={
 qgis:defaultDockLayout,
 map:{...defaultDockLayout,browser:false,legend:false,icons:"small"},
 analysis:{...defaultDockLayout,layers:"right",inspector:"left",table:"right",tabs:true}
};
const dockLayoutKeys:(keyof DockLayout)[]=["layers","inspector","table","browser","legend","tabs","ribbon","tools","icons"];
export function getDockLayoutPreset(layout:DockLayout):DockLayoutPreset|"custom"{
 for(const id of Object.keys(dockLayoutPresets) as DockLayoutPreset[])if(dockLayoutKeys.every(key=>layout[key]===dockLayoutPresets[id][key]))return id;
 return "custom";
}

export function cleanDockLayout(x:unknown):DockLayout{const v=(x&&typeof x==="object"?x:{}) as Partial<DockLayout>;return {layers:v.layers==="right"?"right":"left",inspector:v.inspector==="left"||v.inspector==="float"?v.inspector:"right",table:v.table==="right"||v.table==="float"?v.table:"bottom",browser:v.browser!==false,legend:v.legend!==false,tabs:v.tabs===true,ribbon:v.ribbon!==false,tools:v.tools!==false,icons:v.icons==="small"||v.icons==="large"?v.icons:"medium"}}
export function useDockLayout(storageKey:string):[DockLayout,(patch:Partial<DockLayout>|null)=>void]{
 const [layout,setLayout]=useState<DockLayout>(()=>{try{return cleanDockLayout(JSON.parse(localStorage.getItem(storageKey)??"null"))}catch{return defaultDockLayout}});
 // Toolbars live outside the work area, so their switches go on <html> as data attributes.
 useEffect(()=>{const d=document.documentElement.dataset;d.ovgRibbon=layout.ribbon?"on":"off";d.ovgTools=layout.tools?"on":"off";d.ovgIcons=layout.icons;return()=>{delete d.ovgRibbon;delete d.ovgTools;delete d.ovgIcons}},[layout.ribbon,layout.tools,layout.icons]);
 const update=(patch:Partial<DockLayout>|null)=>setLayout(prev=>{const next=patch?{...prev,...patch}:defaultDockLayout;try{localStorage.setItem(storageKey,JSON.stringify(next))}catch{/* storage unavailable */}requestAnimationFrame(()=>window.dispatchEvent(new Event("resize")));return next});
 return [layout,update];
}
type Panels={layers:boolean;table:boolean};
export function DockLayoutMenu({layout,set,panels,toggleLayers,toggleTable}:{layout:DockLayout;set:(patch:Partial<DockLayout>|null)=>void;panels:Panels;toggleLayers:()=>void;toggleTable:()=>void}){
 const [open,setOpen]=useState(false),ref=useRef<HTMLDivElement>(null),pop=useRef<HTMLDivElement>(null),[at,setAt]=useState({left:4,top:4,maxWidth:280,maxHeight:320}),[draggingPanel,setDraggingPanel]=useState<DockPanelId|null>(null),[dropTarget,setDropTarget]=useState<DockTarget|null>(null);
 // Portal to <body>: the toolbar sits under the layer dock and map overlays, which covered the menu.
 const toggle=()=>setOpen(v=>!v);
 useEffect(()=>{if(!open)return;const click=(e:PointerEvent)=>{if(!ref.current?.contains(e.target as Node)&&!pop.current?.contains(e.target as Node))setOpen(false)};const key=(e:KeyboardEvent)=>{if(e.key==="Escape"){e.preventDefault();setOpen(false);ref.current?.querySelector("button")?.focus()}};const shut=(e:Event)=>{if(!(e.target instanceof Node)||!pop.current?.contains(e.target))setOpen(false)},place=()=>{const r=ref.current?.getBoundingClientRect();if(!r)return;const box=pop.current?.getBoundingClientRect();setAt(fitDockMenuPosition({left:r.left,top:r.top,bottom:r.bottom},{width:box?.width??280,height:box?.height??320},{width:innerWidth,height:innerHeight}))};const frame=requestAnimationFrame(()=>{place();pop.current?.querySelector("select")?.focus({preventScroll:true})});document.addEventListener("pointerdown",click);document.addEventListener("keydown",key);window.addEventListener("resize",place);window.addEventListener("scroll",shut,true);return()=>{cancelAnimationFrame(frame);document.removeEventListener("pointerdown",click);document.removeEventListener("keydown",key);window.removeEventListener("resize",place);window.removeEventListener("scroll",shut,true)}},[open]);
 const side=<K extends "layers"|"inspector"|"table">(k:K,label:string,options:[DockLayout[K],string][])=><fieldset className="dock-menu-row"><legend>{label}</legend>{options.map(([v,t])=><label key={String(v)}><input type="radio" name={"dock-"+k} checked={layout[k]===v} onChange={()=>set({[k]:v} as Partial<DockLayout>)}/>{t}</label>)}</fieldset>;
 const panelNames:Record<DockPanelId,string>={layers:"Khung Lớp",inspector:"Khung thông tin",table:"Bảng thuộc tính"};
 const targetNames:Record<DockTarget,string>={left:"Trái",right:"Phải",bottom:"Dưới",float:"Nổi"};
 const startPanelDrag=(e:React.DragEvent<HTMLSpanElement>,panel:DockPanelId)=>{setDraggingPanel(panel);setDropTarget(null);e.dataTransfer.effectAllowed="move";e.dataTransfer.setData("application/x-ovg-dock-panel",panel)};
 const endPanelDrag=()=>{setDraggingPanel(null);setDropTarget(null)};
 const allowPanelDrop=(e:React.DragEvent<HTMLDivElement>,target:DockTarget)=>{const panel=(e.dataTransfer.getData("application/x-ovg-dock-panel")||draggingPanel) as DockPanelId|null;if(!panel||!dockPanelPatch(panel,target))return;e.preventDefault();e.dataTransfer.dropEffect="move";setDropTarget(target)};
 const finishPanelDrop=(e:React.DragEvent<HTMLDivElement>,target:DockTarget)=>{const panel=(e.dataTransfer.getData("application/x-ovg-dock-panel")||draggingPanel) as DockPanelId|null;if(!panel)return;const patch=dockPanelPatch(panel,target);if(!patch)return;e.preventDefault();set(patch);endPanelDrag()};
 return <div className="dock-menu" ref={ref}><Button type="button" className="plugin-tool" aria-expanded={open} aria-haspopup="dialog" onClick={toggle} title="Bố cục khung: vị trí, ẩn/hiện">▦ Bố cục</Button>{open&&createPortal(<div ref={pop} className="dock-menu-pop ovg-ui-scroll" style={{left:at.left,top:at.top,maxHeight:at.maxHeight,maxWidth:at.maxWidth,overflowY:"auto"}} role="dialog" aria-label="Bố cục khung">
  <label className="dock-menu-preset"><span>Bố cục mẫu</span><Select aria-label="Bố cục mẫu" value={getDockLayoutPreset(layout)} onChange={e=>{const id=e.target.value as DockLayoutPreset;if(id in dockLayoutPresets)set({...dockLayoutPresets[id]})}}>
   <option value="custom" disabled>Tùy chỉnh</option><option value="qgis">QGIS chuẩn</option><option value="map">Ưu tiên bản đồ</option><option value="analysis">Phân tích dữ liệu</option>
  </Select></label>
  <b>Khung hiển thị</b>
  <label><input type="checkbox" checked={panels.layers} onChange={toggleLayers}/> Lớp dữ liệu</label>
  <label><input type="checkbox" checked={layout.browser} onChange={e=>set({browser:e.target.checked})}/> Trình duyệt</label>
  <label><input type="checkbox" checked={layout.legend} onChange={e=>set({legend:e.target.checked})}/> Chú giải nghiệp vụ</label>
  <label><input type="checkbox" checked={panels.table} onChange={toggleTable}/> Bảng thuộc tính</label>
  <b>Thanh công cụ</b>
  <label><input type="checkbox" checked={layout.ribbon} onChange={e=>set({ribbon:e.target.checked})}/> Thanh menu (Bản đồ, Layer, Biên tập…)</label>
  <label><input type="checkbox" checked={layout.tools} onChange={e=>set({tools:e.target.checked})}/> Thanh nút công cụ</label>
  <fieldset className="dock-menu-row"><legend>Cỡ biểu tượng</legend>{([["small","Nhỏ"],["medium","Vừa"],["large","Lớn"]] as const).map(([v,t])=><label key={v}><input type="radio" name="dock-icons" checked={layout.icons===v} onChange={()=>set({icons:v})}/>{t}</label>)}</fieldset>
  <b>Vị trí gắn</b>
  <div className="dock-panel-move" aria-label="Thẻ panel kéo thả">
   <div>{([["layers","Khung Lớp"],["inspector","Khung thông tin"],["table","Bảng thuộc tính"]] as const).map(([panel,label])=><span key={panel} draggable aria-label={"Kéo "+label+" tới một vị trí gắn"} onDragStart={e=>startPanelDrag(e,panel)} onDragEnd={endPanelDrag} style={{display:"inline-flex",alignItems:"center",gap:4,margin:2,padding:"4px 6px",border:"1px dashed currentColor",borderRadius:4,cursor:"grab",userSelect:"none"}}><span aria-hidden="true">⠿</span>{label}</span>)}</div>
   {draggingPanel&&<div className="dock-drop-zones" role="group" aria-label={"Vị trí gắn "+panelNames[draggingPanel]}>{dockTargetsFor(draggingPanel).map(target=><div key={target} className={dropTarget===target?"dock-drop-zone active":"dock-drop-zone"} data-dock-target={target} onDragOver={e=>allowPanelDrop(e,target)} onDragLeave={()=>setDropTarget(v=>v===target?null:v)} onDrop={e=>finishPanelDrop(e,target)} style={{display:"inline-block",margin:2,padding:"6px 8px",border:dropTarget===target?"2px solid currentColor":"1px dashed currentColor",borderRadius:4,minWidth:52,textAlign:"center"}}>{targetNames[target]}</div>)}</div>}
   <small>Kéo thẻ tới vị trí phù hợp. Dùng các lựa chọn radio bên dưới bằng bàn phím.</small>
  </div>
  {side("layers","Khung Lớp",[["left","Trái"],["right","Phải"]])}
  {side("inspector","Khung thông tin",[["right","Phải"],["left","Trái"],["float","Nổi"]])}
  {side("table","Bảng thuộc tính",[["bottom","Dưới"],["right","Phải"],["float","Nổi"]])}
  <label><input type="checkbox" checked={layout.tabs} onChange={e=>set({tabs:e.target.checked})}/> Khung Lớp dạng tab (Lớp · Trình duyệt · Chú giải)</label>
  <Button type="button" onClick={()=>{set(null);setOpen(false);ref.current?.querySelector("button")?.focus({preventScroll:true})}}>Khôi phục bố cục mặc định</Button>
 </div>,document.body)}</div>;
}

// Floating panel: drag the title strip to move, drag the corner to resize (CSS resize), ⤓ docks it back.
export function FloatHandle({storageKey,title,dock}:{storageKey:string;title:string;dock:()=>void}){
 const ref=useRef<HTMLDivElement>(null);
 const place=(x:number,y:number)=>{const p=ref.current?.parentElement;if(!p)return;const r=p.getBoundingClientRect();if(!r.width||!r.height)return;const {x:nx,y:ny}=fitFloatingPanel(x,y,r.width,r.height,innerWidth,innerHeight);p.style.setProperty("--ovg-fx",nx+"px");p.style.setProperty("--ovg-fy",ny+"px");return [nx,ny,r.width,r.height]};
 useEffect(()=>{const p=ref.current?.parentElement;if(!p)return;
  try{const v=JSON.parse(localStorage.getItem(storageKey)??"null");if(Array.isArray(v)&&v.length===4&&v.every(Number.isFinite)&&v[2]>120&&v[3]>80){p.style.width=Math.min(v[2],innerWidth)+"px";p.style.height=Math.min(v[3],innerHeight)+"px";place(v[0],v[1])}}catch{/* ignore */}
  const resize=()=>{const r=p.getBoundingClientRect();place(r.left,r.top)};resize();window.addEventListener("resize",resize);return()=>window.removeEventListener("resize",resize)
 },[storageKey]);
 const save=()=>{const p=ref.current?.parentElement;if(!p)return;const r=p.getBoundingClientRect();if(!r.width||!r.height)return;try{localStorage.setItem(storageKey,JSON.stringify([r.left,r.top,r.width,r.height]))}catch{/* storage unavailable */}};
 // On dock-back the same element stays mounted: drop the floating size/position it carried.
 useEffect(()=>{const p=ref.current?.parentElement;if(!p)return;const o=new ResizeObserver(()=>{const r=p.getBoundingClientRect();place(r.left,r.top);save()});o.observe(p);return()=>{o.disconnect();for(const k of ["width","height","--ovg-fx","--ovg-fy"])p.style.removeProperty(k);requestAnimationFrame(()=>window.dispatchEvent(new Event("resize")))}},[storageKey]);
 const start=(e:React.PointerEvent<HTMLDivElement>)=>{if((e.target as HTMLElement).closest("button"))return;e.preventDefault();const p=ref.current?.parentElement;if(!p)return;const r=p.getBoundingClientRect(),dx=e.clientX-r.left,dy=e.clientY-r.top,el=e.currentTarget;el.setPointerCapture(e.pointerId);
  const move=(ev:PointerEvent)=>place(ev.clientX-dx,ev.clientY-dy);const end=()=>{save();el.removeEventListener("pointermove",move);el.removeEventListener("pointerup",end);el.removeEventListener("pointercancel",end);el.removeEventListener("lostpointercapture",end)};
  el.addEventListener("pointermove",move);el.addEventListener("pointerup",end);el.addEventListener("pointercancel",end);el.addEventListener("lostpointercapture",end,{once:true})};
 return <div ref={ref} className="ovg-float-bar" tabIndex={0} aria-label={"Di chuyển "+title+" bằng phím mũi tên"} onKeyDown={e=>{if(e.target!==e.currentTarget)return;const delta:Record<string,[number,number]>={ArrowLeft:[-20,0],ArrowRight:[20,0],ArrowUp:[0,-20],ArrowDown:[0,20]};const d=delta[e.key],r=ref.current?.parentElement?.getBoundingClientRect();if(d&&r){e.preventDefault();place(r.left+d[0],r.top+d[1]);save()}}} onPointerDown={start} title="Kéo để di chuyển · kéo góc dưới phải để đổi kích thước"><span aria-hidden="true">⠿</span><b>{title}</b><Button type="button" onClick={dock} aria-label={"Gắn lại "+title} title="Gắn lại vào bố cục">⤓</Button></div>;
}
