import {useState,useRef,useEffect} from 'react';
import {Button,Input,Field} from '../primitives';
// Presentation-only subset of SPEC075. Host supplies already verified and parsed session.
// No manifest signature translation, no transport, no extra command or capability.
export type UniversalNode={id:string;kind:'text'|'input'|'button'|'group';label:string;binding?:string|null;children:readonly UniversalNode[]};
export type UniversalSession={scopeId:string;sessionId:string;nodes:readonly UniversalNode[];actions:ReadonlySet<string>;fields:ReadonlySet<string>;invoke:(action:string,values:Readonly<Record<string,string>>)=>Promise<unknown>;isCurrent:()=>boolean};
export function validatePresentation(session:UniversalSession){
 if(!session.scopeId||!session.sessionId||session.actions.size>32||session.fields.size>32)return false;
 const ids=new Set<string>(),token=/^[a-zA-Z0-9_.-]{1,64}$/;let count=0,bytes=0;
 const walk=(nodes:readonly UniversalNode[],depth:number):boolean=>{if(depth>16||!Array.isArray(nodes))return false;for(const n of nodes){if(!n||++count>500||!token.test(n.id)||ids.has(n.id)||typeof n.label!=='string'||n.label.length>2000||!Array.isArray(n.children))return false;ids.add(n.id);bytes+=new TextEncoder().encode(n.label).length;if(bytes>256*1024)return false;
 if(n.kind==='group'){if(n.binding!=null||!walk(n.children,depth+1))return false}
 else if(n.children.length||!(n.kind==='text'&&n.binding==null||n.kind==='input'&&typeof n.binding==='string'&&session.fields.has(n.binding)||n.kind==='button'&&typeof n.binding==='string'&&session.actions.has(n.binding)))return false;}return true};
 return walk(session.nodes,1);
}
export function PluginPanel({session}:{session:UniversalSession}){return <SessionPanel key={JSON.stringify([session.scopeId,session.sessionId])} session={session}/>}
function SessionPanel({session}:{session:UniversalSession}){
 const [values,setValues]=useState<Record<string,string>>({}),[busy,setBusy]=useState(false),[output,setOutput]=useState('');const flight=useRef(false),alive=useRef(true),latest=useRef(session);latest.current=session;
 useEffect(()=>{alive.current=true;return()=>{alive.current=false}},[]);
 const invoke=async(id:string)=>{if(flight.current||!session.isCurrent()||!session.actions.has(id))return;flight.current=true;setBusy(true);setOutput('');try{const args=Object.fromEntries(Object.entries(values).filter(([k,v])=>session.fields.has(k)&&v.length<=4096));const result=await session.invoke(id,args);if(alive.current&&latest.current===session&&session.isCurrent())setOutput(String(JSON.stringify(result)??'').slice(0,32768))}catch{if(alive.current&&latest.current===session&&session.isCurrent())setOutput('Action denied')}finally{flight.current=false;if(alive.current&&latest.current===session)setBusy(false)}};
 if(!validatePresentation(session)||!session.isCurrent())return <p role="alert">Plugin unavailable</p>;
 const render=(n:UniversalNode):React.ReactNode=>{switch(n.kind){case 'text':return <p key={n.id}>{n.label}</p>;case 'group':return <section key={n.id}>{n.children.map(render)}</section>;case 'input':return <Field key={n.id} label={n.label}>{p=><Input {...p} maxLength={4096} disabled={busy} value={values[n.binding!]??''} onChange={e=>setValues(v=>({...v,[n.binding!]:e.target.value}))}/>}</Field>;case 'button':return <Button key={n.id} disabled={busy} onClick={()=>void invoke(n.binding!)}>{n.label}</Button>}};
 return <section aria-label="Read-only plugin" aria-busy={busy}>{session.nodes.map(render)}<output>{output}</output></section>;
}