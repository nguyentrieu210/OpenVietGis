import React,{useEffect,useRef,useState} from "react";
import {useDesktopDialog} from "./studio-ui";
import {createRoot} from "react-dom/client";

// Spec 058: in-app replacements for window.prompt / confirm / alert (styled like the rest of the app,
// keyboard friendly: Enter = OK, Esc = Cancel). Each call mounts one small dialog and resolves on close.
type Opts={title?:string;okLabel?:string;cancelLabel?:string;danger?:boolean};
function mount<T>(render:(done:(v:T)=>void)=>React.ReactElement):Promise<T>{
 return new Promise(resolve=>{const host=document.createElement("div");host.className="qgis-studio ovg-dialog-host";document.body.appendChild(host);const root=createRoot(host);
  const done=(v:T)=>{root.unmount();host.remove();resolve(v)};root.render(render(done))});
}
function Shell({title,children,onCancel}:{title:string;children:React.ReactNode;onCancel:()=>void}){
 const panel=useRef<HTMLDivElement>(null);useDesktopDialog(panel,onCancel);
 return <div className="ovg-dialog-shade" onMouseDown={e=>{if(e.target===e.currentTarget)onCancel()}}><div ref={panel} tabIndex={-1} className="ovg-dialog" role="dialog" aria-modal="true" aria-label={title}><header>{title}</header>{children}</div></div>;
}
function Ask({message,initial,opts,done}:{message:string;initial:string;opts:Opts;done:(v:string|null)=>void}){
 const [v,setV]=useState(initial),ref=useRef<HTMLInputElement>(null);useEffect(()=>{ref.current?.focus();ref.current?.select()},[]);
 return <Shell title={opts.title??"Nhập"} onCancel={()=>done(null)}><form onSubmit={e=>{e.preventDefault();done(v)}}><label>{message}<input ref={ref} value={v} onChange={e=>setV(e.target.value)}/></label><div className="ovg-dialog-buttons"><button type="button" onClick={()=>done(null)}>{opts.cancelLabel??"Hủy"}</button><button type="submit" className="primary">{opts.okLabel??"OK"}</button></div></form></Shell>;
}
function Confirm({message,opts,done}:{message:string;opts:Opts;done:(v:boolean)=>void}){
 const ok=useRef<HTMLButtonElement>(null),cancel=useRef<HTMLButtonElement>(null);useEffect(()=>{(opts.danger?cancel:ok).current?.focus()},[opts.danger]);
 return <Shell title={opts.title??"Xác nhận"} onCancel={()=>done(false)}><p>{message}</p><div className="ovg-dialog-buttons"><button ref={cancel} type="button" onClick={()=>done(false)}>{opts.cancelLabel??"Hủy"}</button><button ref={ok} type="button" className={opts.danger?"danger-solid":"primary"} onClick={()=>done(true)}>{opts.okLabel??"Đồng ý"}</button></div></Shell>;
}
export const uiPrompt=(message:string,initial="",opts:Opts={})=>mount<string|null>(done=><Ask message={message} initial={initial} opts={opts} done={done}/>);
export const uiConfirm=(message:string,opts:Opts={})=>mount<boolean>(done=><Confirm message={message} opts={opts} done={done}/>);
export const uiAlert=(message:string,opts:Opts={})=>mount<boolean>(done=><Confirm message={message} opts={{okLabel:"Đóng",...opts,title:opts.title??"Thông báo"}} done={done}/>).then(()=>undefined);
