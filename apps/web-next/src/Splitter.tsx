import React,{useEffect,useRef,useState} from "react";
import {clampSize,separatorKeySize} from "./dock-sizing";
export {clampSize} from "./dock-sizing";

// Resize the existing dock without rerendering the map on every pointer move.
type Props={storageKey:string;cssVar:string;axis:"x"|"y";edge:"start"|"end";min:number;max:number;label:string;defaultSize?:number};
const read=(key:string)=>{try{const v=Number(localStorage.getItem(key));return Number.isFinite(v)&&v>0?v:null}catch{return null}};
const write=(key:string,v:number|null)=>{try{if(v===null)localStorage.removeItem(key);else localStorage.setItem(key,String(Math.round(v)))}catch{/* storage unavailable */}};

export function Splitter({storageKey,cssVar,axis,edge,min,max,label,defaultSize}:Props){
 const ref=useRef<HTMLDivElement>(null),abort=useRef<(()=>void)|null>(null);
 const [range,setRange]=useState({min,max,now:clampSize(defaultSize??min,min,max)});
 const limit=()=>Math.max(0,Math.min(max,(axis==="x"?innerWidth:innerHeight)*.8));
 const size=()=>{const r=ref.current?.parentElement?.getBoundingClientRect();return r?(axis==="x"?r.width:r.height):defaultSize??min};
 const sync=()=>{const upper=limit(),lower=Math.min(Math.max(0,min),upper),now=Math.round(clampSize(size(),lower,upper));setRange({min:Math.floor(lower),max:Math.ceil(upper),now})};
 const apply=(v:number|null)=>{const panel=ref.current?.parentElement;if(!panel)return;if(v===null)panel.style.removeProperty(cssVar);else panel.style.setProperty(cssVar,clampSize(v,min,limit())+"px");sync()};
 useEffect(()=>{const resize=()=>apply(read(storageKey));resize();window.addEventListener("resize",resize);return()=>{window.removeEventListener("resize",resize);abort.current?.()}},[storageKey,cssVar,axis,min,max,defaultSize]);
 const start=(e:React.PointerEvent<HTMLDivElement>)=>{
  if(e.button!==0||!e.isPrimary||abort.current)return;
  e.preventDefault();const el=e.currentTarget,pointer=e.pointerId,from=axis==="x"?e.clientX:e.clientY,base=size(),sign=edge==="start"?-1:1;
  el.focus({preventScroll:true});try{el.setPointerCapture(pointer)}catch{/* window listeners remain active */}
  let last=base,moved=false,done=false;
  const ghost=document.createElement("div");ghost.className="ovg-split-ghost ovg-split-ghost-"+axis;document.body.appendChild(ghost);
  const place=(v:number)=>{if(axis==="x")ghost.style.left=v-1+"px";else ghost.style.top=v-1+"px"};place(from);
  const move=(ev:PointerEvent)=>{if(ev.pointerId!==pointer)return;const delta=(axis==="x"?ev.clientX:ev.clientY)-from;moved=moved||delta!==0;last=clampSize(base+sign*delta,min,limit());place(from+sign*(last-base))};
  const finish=(commit:boolean)=>{if(done)return;done=true;abort.current=null;ghost.remove();
   window.removeEventListener("pointermove",move);window.removeEventListener("pointerup",up);window.removeEventListener("pointercancel",cancel);el.removeEventListener("lostpointercapture",cancel);window.removeEventListener("blur",stop);window.removeEventListener("keydown",escape,true);
   try{if(el.hasPointerCapture(pointer))el.releasePointerCapture(pointer)}catch{/* capture already lost */}
   if(commit&&moved){apply(last);write(storageKey,last);window.dispatchEvent(new Event("resize"))}
  };
  const up=(ev:PointerEvent)=>{if(ev.pointerId===pointer){move(ev);finish(true)}};
  const cancel=(ev:PointerEvent)=>{if(ev.pointerId===pointer)finish(false)};
  const stop=()=>finish(false);
  const escape=(ev:KeyboardEvent)=>{if(ev.key==="Escape"){ev.preventDefault();ev.stopPropagation();finish(false)}};
  abort.current=stop;
  window.addEventListener("pointermove",move);window.addEventListener("pointerup",up);window.addEventListener("pointercancel",cancel);el.addEventListener("lostpointercapture",cancel);window.addEventListener("blur",stop);window.addEventListener("keydown",escape,true);
 };
 const key=(e:React.KeyboardEvent)=>{if(e.altKey||e.ctrlKey||e.metaKey||e.nativeEvent.isComposing)return;const v=separatorKeySize(e.key,axis,edge,size(),min,limit(),e.shiftKey?5:20);if(v===null)return;e.preventDefault();e.stopPropagation();apply(v);write(storageKey,v);window.dispatchEvent(new Event("resize"))};
 const reset=()=>{apply(null);write(storageKey,null);window.dispatchEvent(new Event("resize"))};
 return <div ref={ref} className={"ovg-splitter ovg-splitter-"+axis+" ovg-splitter-"+edge} role="separator" aria-orientation={axis==="x"?"vertical":"horizontal"} aria-valuemin={range.min} aria-valuemax={range.max} aria-valuenow={range.now} aria-label={label} aria-keyshortcuts="ArrowLeft ArrowRight ArrowUp ArrowDown Home End" title={label+" · kéo hoặc dùng phím mũi tên; Home/End: giới hạn; nhấp đúp: mặc định"} tabIndex={0} onPointerDown={start} onDoubleClick={reset} onKeyDown={key}/>;
}
