import {useState} from 'react';
import {Dialog} from '../overlays';
import {Button,Input} from '../primitives';
import {CommandRegistry,type CommandContext} from './index';
export function CommandPalette({registry,context,open,onOpenChange}:{registry:CommandRegistry;context:()=>CommandContext;open:boolean;onOpenChange:(v:boolean)=>void}){
 const [q,setQ]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState('');
 const items=registry.list(context()).filter(x=>[x.id,x.label,x.group].some(v=>v.toLowerCase().includes(q.toLowerCase())));
 return <Dialog open={open} onOpenChange={onOpenChange} title={context().locale==='vi'?'Lệnh':'Commands'}><Input aria-label="Search commands" value={q} onChange={e=>setQ(e.target.value)}/>{items.slice(0,100).map(c=><div key={c.id}><Button disabled={busy||!!c.reason} onClick={async()=>{setBusy(true);setError('');try{await registry.execute(c.id,context);onOpenChange(false)}catch{setError('Command denied')}finally{setBusy(false)}}}>{c.label} {c.shortcut} {c.reason}</Button></div>)}{error&&<p role="alert">{error}</p>}</Dialog>;
}