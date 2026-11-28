import React,{useEffect,useRef,useState,type ReactNode} from "react";
import {Splitter} from "./Splitter";
import type {PluginAction} from "./plugin-ui-registry";
import "./plugin-workspace.css";
type Side="right"|"left"|"float";
const read=(key:string)=>{try{return localStorage.getItem(key)}catch{return null}};
const save=(key:string,value:string)=>{try{localStorage.setItem(key,value)}catch{}};
export function PluginWorkspace({scope,title,close,children}:{scope:string;title:string;close:()=>void;children:ReactNode}){
 const [side,setSide]=useState<Side>(()=>{const v=read(scope+":side");return v==="left"||v==="float"?v:"right"});
 const [collapsed,setCollapsed]=useState(false);
 const change=(v:Side)=>{setSide(v);save(scope+":side",v)};
 return <aside className={"ovg-plugin-workspace side-"+side+(collapsed?" collapsed":"")} aria-label={title}>
 {!collapsed&&<Splitter storageKey={scope+":width"} cssVar="--ovg-plugin-width" axis="x" edge={side==="left"?"end":"start"} min={270} max={760} defaultSize={380} label="Chiều rộng không gian plugin"/>}
 <header><b>{title}</b><select aria-label="Vị trí khung plugin" value={side} onChange={e=>change(e.target.value as Side)}><option value="right">Phải</option><option value="left">Trái</option><option value="float">Nổi</option></select><button aria-label={collapsed?"Mở rộng plugin":"Thu gọn plugin"} onClick={()=>setCollapsed(x=>!x)}>{collapsed?"+":"−"}</button><button aria-label="Đóng plugin" onClick={close}>×</button></header>
 {!collapsed&&<div className="ovg-plugin-scroll">{children}</div>}
 </aside>
}
export function PluginToolPalette({scope,actions,close}:{scope:string;actions:readonly PluginAction[];close:()=>void}){
 const [pos,setPos]=useState(()=>{try{const v=JSON.parse(read(scope+":position")??"null");if(Number.isFinite(v?.x)&&Number.isFinite(v?.y))return {x:Math.max(0,v.x),y:Math.max(0,v.y)}}catch{}return {x:16,y:70}});
 const root=useRef<HTMLDivElement>(null),drag=useRef<{id:number;x:number;y:number;px:number;py:number}|null>(null);
 const clamp=(p:{x:number;y:number})=>{const host=root.current?.parentElement,b=root.current?.getBoundingClientRect();return {x:Math.max(0,Math.min(p.x,Math.max(0,(host?.clientWidth??innerWidth)-(b?.width??160)))),y:Math.max(0,Math.min(p.y,Math.max(0,(host?.clientHeight??innerHeight)-(b?.height??40))))}};
 useEffect(()=>{const el=root.current?.parentElement;if(!el)return;const observer=new ResizeObserver(()=>setPos(p=>clamp(p)));observer.observe(el);return()=>observer.disconnect()},[]);
 return <div ref={root} className="ovg-plugin-palette" role="toolbar" aria-label="Công cụ plugin" style={{left:pos.x,top:pos.y}}><button className="ovg-plugin-grip" aria-label="Di chuyển thanh công cụ plugin" title="Kéo để di chuyển" onPointerDown={e=>{if(e.button!==0)return;drag.current={id:e.pointerId,x:e.clientX,y:e.clientY,px:pos.x,py:pos.y};e.currentTarget.setPointerCapture(e.pointerId)}} onPointerMove={e=>{const d=drag.current;if(d?.id===e.pointerId)setPos(clamp({x:d.px+e.clientX-d.x,y:d.py+e.clientY-d.y}))}} onPointerUp={e=>{if(drag.current?.id===e.pointerId){drag.current=null;save(scope+":position",JSON.stringify(pos))}}} onPointerCancel={()=>{drag.current=null}} onKeyDown={e=>{const dx=e.key==="ArrowLeft"?-20:e.key==="ArrowRight"?20:0,dy=e.key==="ArrowUp"?-20:e.key==="ArrowDown"?20:0;if(dx||dy){e.preventDefault();const next=clamp({x:pos.x+dx,y:pos.y+dy});setPos(next);save(scope+":position",JSON.stringify(next))}}}>⠿</button>{actions.filter(a=>a.toolbar).map(a=><button key={a.id} disabled={a.disabled} title={a.label} aria-label={a.label} onClick={a.run}>{a.shortLabel??a.label}</button>)}<button aria-label="Ẩn thanh công cụ plugin" onClick={close}>×</button></div>
}
