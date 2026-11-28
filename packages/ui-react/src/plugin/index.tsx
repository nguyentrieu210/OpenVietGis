import {useState,useRef,useEffect} from 'react';
import {PluginDocumentView,validateDocumentPresentation,type UniversalNode} from './document-view';
export {PluginDocumentView,validateDocumentPresentation,type UniversalNode,type DocumentPresentation} from './document-view';
export type UniversalSession={scopeId:string;sessionId:string;nodes:readonly UniversalNode[];actions:ReadonlySet<string>;fields:ReadonlySet<string>;invoke:(action:string,values:Readonly<Record<string,string>>)=>Promise<unknown>;isCurrent:()=>boolean};
export function validatePresentation(session:UniversalSession){return !!session.scopeId&&!!session.sessionId&&validateDocumentPresentation(session)}
export function PluginPanel({session}:{session:UniversalSession}){return <SessionPanel key={JSON.stringify([session.scopeId,session.sessionId])} session={session}/>}
function SessionPanel({session}:{session:UniversalSession}){
 const [values,setValues]=useState<Record<string,string>>({}),[busy,setBusy]=useState(false),[output,setOutput]=useState('');const flight=useRef(false),alive=useRef(true),latest=useRef(session);latest.current=session;
 useEffect(()=>{alive.current=true;return()=>{alive.current=false}},[]);
 const invoke=async(id:string)=>{if(flight.current||!session.isCurrent()||!session.actions.has(id))return;flight.current=true;setBusy(true);setOutput('');try{const args=Object.fromEntries(Object.entries(values).filter(([k,v])=>session.fields.has(k)&&v.length<=4096));const result=await session.invoke(id,args);if(alive.current&&latest.current===session&&session.isCurrent())setOutput(String(JSON.stringify(result)??'').slice(0,32768))}catch{if(alive.current&&latest.current===session&&session.isCurrent())setOutput('Action denied')}finally{flight.current=false;if(alive.current&&latest.current===session)setBusy(false)}};
 if(!validatePresentation(session)||!session.isCurrent())return <p role="alert">Plugin unavailable</p>;
 return <section aria-label="Read-only plugin" aria-busy={busy}><PluginDocumentView document={session} values={values} busy={busy} onValue={(field,value)=>setValues(v=>({...v,[field]:value}))} onAction={id=>void invoke(id)}/><output>{output}</output></section>;
}