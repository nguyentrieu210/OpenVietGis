import React,{useEffect,useRef} from "react";

// Spec 057: QGIS-style resizable panels. The splitter sets a CSS variable on its parent panel
// (width for side docks, height for bottom/stacked docks) and remembers it per browser.
type Props={storageKey:string;cssVar:string;axis:"x"|"y";edge:"start"|"end";min:number;max:number;label:string;defaultSize?:number};
const read=(key:string)=>{try{const v=Number(localStorage.getItem(key));return Number.isFinite(v)&&v>0?v:null}catch{return null}};
const write=(key:string,v:number|null)=>{try{if(v===null)localStorage.removeItem(key);else localStorage.setItem(key,String(Math.round(v)))}catch{/* storage unavailable */}};
export const clampSize=(v:number,min:number,max:number)=>Math.max(min,Math.min(max,v));

export function Splitter({storageKey,cssVar,axis,edge,min,max,label,defaultSize}:Props){
 const ref=useRef<HTMLDivElement>(null);
 const limit=()=>Math.min(max,(axis==="x"?innerWidth:innerHeight)*.8);
 const apply=(v:number|null)=>{const panel=ref.current?.parentElement;if(!panel)return;if(v===null)panel.style.removeProperty(cssVar);else panel.style.setProperty(cssVar,clampSize(v,min,limit())+"px")};
 useEffect(()=>{apply(read(storageKey))},[storageKey]);
 const size=()=>{const r=ref.current?.parentElement?.getBoundingClientRect();return r?(axis==="x"?r.width:r.height):defaultSize??min};
 const start=(e:React.PointerEvent<HTMLDivElement>)=>{e.preventDefault();const el=e.currentTarget,from=axis==="x"?e.clientX:e.clientY,base=size(),sign=edge==="start"?-1:1;el.setPointerCapture(e.pointerId);let last=base,moved=false;
  const move=(ev:PointerEvent)=>{moved=true;last=clampSize(base+sign*((axis==="x"?ev.clientX:ev.clientY)-from),min,limit());apply(last)};
  const end=()=>{if(moved)write(storageKey,last);el.removeEventListener("pointermove",move);el.removeEventListener("pointerup",end);el.removeEventListener("pointercancel",end);window.dispatchEvent(new Event("resize"))};
  el.addEventListener("pointermove",move);el.addEventListener("pointerup",end);el.addEventListener("pointercancel",end)};
 const key=(e:React.KeyboardEvent)=>{const grow=axis==="x"?(edge==="start"?"ArrowLeft":"ArrowRight"):(edge==="start"?"ArrowUp":"ArrowDown"),shrink=axis==="x"?(edge==="start"?"ArrowRight":"ArrowLeft"):(edge==="start"?"ArrowDown":"ArrowUp");if(e.key!==grow&&e.key!==shrink)return;e.preventDefault();const v=clampSize(size()+(e.key===grow?20:-20),min,limit());apply(v);write(storageKey,v);window.dispatchEvent(new Event("resize"))};
 const reset=()=>{apply(null);write(storageKey,null);window.dispatchEvent(new Event("resize"))};
 return <div ref={ref} className={"ovg-splitter ovg-splitter-"+axis+" ovg-splitter-"+edge} role="separator" aria-orientation={axis==="x"?"vertical":"horizontal"} aria-label={label+" · kéo để đổi kích thước, nhấp đúp để về mặc định"} title={label+" · kéo để đổi kích thước, nhấp đúp để về mặc định"} tabIndex={0} onPointerDown={start} onDoubleClick={reset} onKeyDown={key}/>;
}
