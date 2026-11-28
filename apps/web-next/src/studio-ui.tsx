import React,{useEffect,useLayoutEffect,useRef,useState,type ReactNode} from "react";
import "./desktop-dialog.css";

// Only the active blocking dialog owns Escape and Tab. Docked tools remain
// non-modal so users can continue working with the map.
const modalStack:HTMLElement[]=[];
const focusable='button:not([disabled]),a[href],input:not([disabled]):not([type=hidden]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';
export function useDesktopDialog(ref:React.RefObject<HTMLElement|null>,close:()=>void,enabled=true){
 const latest=useRef(close);latest.current=close;
 useLayoutEffect(()=>{
  const el=ref.current;if(!enabled||!el)return;
  const previous=document.activeElement instanceof HTMLElement?document.activeElement:null;
  modalStack.push(el);
  const items=()=>Array.from(el.querySelectorAll<HTMLElement>(focusable)).filter(x=>x.tabIndex>=0&&!x.matches(":disabled")&&!x.closest('[inert],[hidden],[aria-hidden="true"]')&&x.getClientRects().length>0&&getComputedStyle(x).visibility!=="hidden").sort((a,b)=>(a.tabIndex||Number.MAX_SAFE_INTEGER)-(b.tabIndex||Number.MAX_SAFE_INTEGER));
  const raf=requestAnimationFrame(()=>{if(top()&&!el.contains(document.activeElement))(el.querySelector<HTMLElement>('[autofocus]')??el).focus({preventScroll:true})});
  const top=()=>modalStack.at(-1)===el&&!Array.from(document.querySelectorAll<HTMLElement>('dialog[open],[aria-modal="true"]')).some(x=>x!==el&&!x.contains(el)&&!el.contains(x)&&!modalStack.includes(x)&&x.getClientRects().length);
  const key=(e:KeyboardEvent)=>{if(!top()||e.isComposing)return;
   if(e.key==="Escape"){e.preventDefault();e.stopImmediatePropagation();latest.current();return}
   if(e.key!=="Tab")return;const all=items(),first=all[0],last=all.at(-1),current=document.activeElement;
   if(!first||!last){e.preventDefault();el.focus();return}
   if(!el.contains(current)||current===el||e.shiftKey&&current===first||!e.shiftKey&&current===last){e.preventDefault();(e.shiftKey?last:first).focus()}
  };
  const contain=(e:FocusEvent)=>{if(top()&&e.target instanceof Node&&!el.contains(e.target)){(items()[0]??el).focus({preventScroll:true})}};
  window.addEventListener("keydown",key,true);document.addEventListener("focusin",contain);
  return()=>{cancelAnimationFrame(raf);window.removeEventListener("keydown",key,true);document.removeEventListener("focusin",contain);const index=modalStack.lastIndexOf(el);if(index>=0)modalStack.splice(index,1);
   requestAnimationFrame(()=>{const current=document.activeElement;if(previous?.isConnected&&(!current||current===document.body||el.contains(current))&&!previous.closest("[inert],[hidden]"))previous.focus({preventScroll:true})})};
 },[ref,enabled]);
}
export function Modal({title,close,children,docked=false}:{title:string;close:()=>void;children:ReactNode;docked?:boolean}){
 const panel=useRef<HTMLElement>(null);
 useDesktopDialog(panel,close,!docked);
 return <div className={"live-shade"+(docked?" partner-docked":"")} onMouseDown={e=>{if(!docked&&e.target===e.currentTarget)close()}}><section ref={panel} tabIndex={-1} className="live-modal panel" role="dialog" aria-modal={!docked} aria-label={title}><header><div><h2>{title}</h2></div><button type="button" aria-label="Đóng" title="Đóng (Esc)" onClick={close}>✕</button></header><div className="live-modal-body">{children}</div></section></div>
}
type Field={name:string;label:string;optional?:boolean;options?:{value:string;label:string}[]};
export function SimpleForm({fields,busy,submit}:{fields:Field[];busy:boolean;submit:(v:Record<string,string>)=>void}){const [v,setV]=useState<Record<string,string>>(()=>Object.fromEntries(fields.map(f=>[f.name,f.optional?"":f.options?.[0]?.value??""])));return <form onSubmit={e=>{e.preventDefault();submit(v)}}>{fields.map(f=><label key={f.name}>{f.label}{f.options?<select required={!f.optional} value={v[f.name]} onChange={e=>setV(w=>({...w,[f.name]:e.target.value}))}><option value="">Chọn…</option>{f.options.map(o=><option key={o.value} value={o.value}>{o.label}</option>)}</select>:<input required={!f.optional} value={v[f.name]} onChange={e=>setV(w=>({...w,[f.name]:e.target.value}))}/>}</label>)}<button className="primary" disabled={busy||fields.some(f=>!f.optional&&!v[f.name]?.trim())}>{busy?"Đang lưu…":"Lưu vào dự án"}</button></form>}
