import type {ReactNode} from 'react';
import {Button,Input,Field} from '../primitives';
export type UniversalNode={id:string;kind:'text'|'input'|'button'|'group';label:string;binding?:string|null;children:readonly UniversalNode[]};
export type DocumentPresentation={nodes:readonly UniversalNode[];actions:ReadonlySet<string>;fields:ReadonlySet<string>};
export function validateDocumentPresentation(doc:DocumentPresentation){
 if(doc.actions.size>32||doc.fields.size>32)return false;
 const ids=new Set<string>(),token=/^[a-zA-Z0-9_.-]{1,64}$/;let count=0,bytes=0;
 const walk=(nodes:readonly UniversalNode[],depth:number):boolean=>{if(depth>16||!Array.isArray(nodes))return false;for(const n of nodes){if(!n||++count>500||!token.test(n.id)||ids.has(n.id)||typeof n.label!=='string'||[...n.label].length>(n.kind==='text'?2000:200)||!Array.isArray(n.children))return false;ids.add(n.id);bytes+=new TextEncoder().encode(n.label).length;if(bytes>256*1024)return false;
 if(n.kind==='group'){if(n.binding!=null||!walk(n.children,depth+1))return false}
 else if(n.children.length||!(n.kind==='text'&&n.binding==null||n.kind==='input'&&typeof n.binding==='string'&&doc.fields.has(n.binding)||n.kind==='button'&&typeof n.binding==='string'&&doc.actions.has(n.binding)))return false;}return true};
 return walk(doc.nodes,1);
}
// Host owns authoritative parsing, capability/session checks, transport,
// action-specific input validation, and human confirmation.
export function PluginDocumentView({document,values,busy,onValue,onAction,unavailableLabel='Plugin unavailable'}:{document:DocumentPresentation;values:Readonly<Record<string,string>>;busy:boolean;onValue:(field:string,value:string)=>void;onAction:(id:string)=>void;unavailableLabel?:string}){
 if(!validateDocumentPresentation(document))return <p role="alert">{unavailableLabel}</p>;
 const render=(n:UniversalNode):ReactNode=>{switch(n.kind){case 'text':return <p key={n.id}>{n.label}</p>;case 'group':return <div key={n.id}>{n.children.map(render)}</div>;case 'input':return <Field key={n.id} label={n.label}>{p=><Input {...p} maxLength={4096} disabled={busy} value={Object.hasOwn(values,n.binding!)?values[n.binding!]??'':''} onChange={e=>{if(!busy&&document.fields.has(n.binding!))onValue(n.binding!,e.target.value)}}/>}</Field>;case 'button':return <Button key={n.id} disabled={busy} onClick={()=>{if(!busy&&document.actions.has(n.binding!))onAction(n.binding!)}}>{n.label}</Button>}};
 return <div className="ovg-plugin-document" aria-busy={busy}>{document.nodes.map(render)}</div>;
}