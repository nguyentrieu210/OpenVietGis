import React,{useEffect,useRef,useState} from "react";
import {clampSize,separatorKeySize} from "./dock-sizing";
export {clampSize} from "./dock-sizing";

// Resize the existing dock without rerendering the map on every pointer move.
type Props={storageKey:string;cssVar:string;axis:"x"|"y";edge:"start"|"end";min:number;max:number;label:string;defaultSize?:number;controlledSize?:number;onSizeCommit?:(size:number)=>void;centered?:boolean;disabled?:boolean};
const read=(key:string)=>{try{const v=Number(localStorage.getItem(key));return Number.isFinite(v)&&v>0?v:null}catch{return null}};
const write=(key:string,v:number|null)=>{try{if(v===null)localStorage.removeItem(key);else localStorage.setItem(key,String(Math.round(v)))}catch{/* storage unavailable */}};

export function Splitter({storageKey,cssVar,axis,edge,min,max,label,defaultSize,controlledSize,onSizeCommit,centered=false,disabled=false}:Props){
 const ref=useRef<HTMLDivElement>(null),abort=useRef<(()=>void)|null>(null);
 const [range,setRange]=useState({min,max,now:clampSize(defaultSize??min,min,max)});
 const limit=()=>{const panel=ref.current?.parentElement,tree=panel?.closest(".qgis-dock")?.querySelector(".qgis-tree");const shared=axis==="y"&&panel&&tree&&["qgis-browser","qgis-basemaps","qgis-legend"].some(c=>panel.classList.contains(c))?Math.max(min,panel.getBoundingClientRect().height+tree.getBoundingClientRect().height-72):Infinity;return Math.max(0,Math.min(max,centered?(axis==="x"?innerWidth:innerHeight)-24:(axis==="x"?innerWidth:innerHeight)*.8,shared))};
 const size=()=>{const r=ref.current?.parentElement?.getBoundingClientRect();return r?(axis==="x"?r.width:r.height):defaultSize??min};
 const sync=()=>{const upper=limit(),lower=Math.min(Math.max(0,min),upper),now=Math.round(clampSize(size(),lower,upper));setRange({min:Math.floor(lower),max:Math.ceil(upper),now})};
 const apply=(v:number|null)=>{const panel=ref.current?.parentElement;if(!panel)return;if(controlledSize!==undefined){if(v!==null)onSizeCommit?.(clampSize(v,min,limit()));else if(defaultSize!==undefined)onSizeCommit?.(clampSize(defaultSize,min,limit()))}else if(v===null)panel.style.removeProperty(cssVar);else panel.style.setProperty(cssVar,clampSize(v,min,limit())+"px");sync()};
 useEffect(()=>{const resize=()=>{if(controlledSize===undefined&&!disabled)apply(read(storageKey));else sync()};resize();window.addEventListener("resize",resize);const observer=new ResizeObserver(()=>sync());if(ref.current?.parentElement)observer.observe(ref.current.parentElement);return()=>{observer.disconnect();window.removeEventListener("resize",resize);abort.current?.()}},[storageKey,cssVar,axis,min,max,defaultSize,controlledSize,disabled]);
 const start=(e:React.PointerEvent<HTMLDivElement>)=>{
  if(disabled||e.button!==0||!e.isPrimary||abort.current)return;
  e.preventDefault();const el=e.currentTarget,pointer=e.pointerId,from=axis==="x"?e.clientX:e.clientY,base=size(),sign=edge==="start"?-1:1;
  el.focus({preventScroll:true});try{el.setPointerCapture(pointer)}catch{/* window listeners remain active */}
  let last=base,moved=false,done=false;
  const ghost=document.createElement("div");ghost.className="ovg-split-ghost ovg-split-ghost-"+axis;document.body.appendChild(ghost);
  const place=(v:number)=>{if(axis==="x")ghost.style.left=v-1+"px";else ghost.style.top=v-1+"px"};place(from);
  const move=(ev:PointerEvent)=>{if(ev.pointerId!==pointer)return;const delta=(axis==="x"?ev.clientX:ev.clientY)-from;moved=moved||delta!==0;const factor=centered?2:1;last=clampSize(base+sign*delta*factor,min,limit());place(from+sign*(last-base)/factor)};
  const finish=(commit:boolean)=>{if(done)return;done=true;abort.current=null;ghost.remove();
   window.removeEventListener("pointermove",move);window.removeEventListener("pointerup",up);window.removeEventListener("pointercancel",cancel);el.removeEventListener("lostpointercapture",cancel);window.removeEventListener("blur",stop);window.removeEventListener("keydown",escape,true);
   try{if(el.hasPointerCapture(pointer))el.releasePointerCapture(pointer)}catch{/* capture already lost */}
   if(commit&&moved){apply(last);if(controlledSize===undefined)write(storageKey,last);window.dispatchEvent(new Event("resize"))}
  };
  const up=(ev:PointerEvent)=>{if(ev.pointerId===pointer){move(ev);finish(true)}};
  const cancel=(ev:PointerEvent)=>{if(ev.pointerId===pointer)finish(false)};
  const stop=()=>finish(false);
  const escape=(ev:KeyboardEvent)=>{if(ev.key==="Escape"){ev.preventDefault();ev.stopPropagation();finish(false)}};
  abort.current=stop;
  window.addEventListener("pointermove",move);window.addEventListener("pointerup",up);window.addEventListener("pointercancel",cancel);el.addEventListener("lostpointercapture",cancel);window.addEventListener("blur",stop);window.addEventListener("keydown",escape,true);
 };
 const key=(e:React.KeyboardEvent)=>{if(disabled||e.altKey||e.ctrlKey||e.metaKey||e.nativeEvent.isComposing)return;const v=separatorKeySize(e.key,axis,edge,size(),min,limit(),e.shiftKey?5:20);if(v===null)return;e.preventDefault();e.stopPropagation();apply(v);if(controlledSize===undefined)write(storageKey,v);window.dispatchEvent(new Event("resize"))};
 const reset=()=>{if(disabled)return;apply(null);if(controlledSize===undefined)write(storageKey,null);window.dispatchEvent(new Event("resize"))};
 return <div ref={ref} className={"ovg-splitter ovg-splitter-"+axis+" ovg-splitter-"+edge} role="separator" aria-disabled={disabled||undefined} aria-orientation={axis==="x"?"vertical":"horizontal"} aria-valuemin={range.min} aria-valuemax={range.max} aria-valuenow={range.now} aria-label={label} aria-keyshortcuts="ArrowLeft ArrowRight ArrowUp ArrowDown Home End" title={label+" · kéo hoặc dùng phím mũi tên; Home/End: giới hạn; nhấp đúp: mặc định"} tabIndex={disabled?-1:0} onPointerDown={start} onDoubleClick={reset} onKeyDown={key}/>;
}
