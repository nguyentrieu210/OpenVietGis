import {useState} from 'react';
import {createRoot} from 'react-dom/client';
import {Button,Input,Select,Textarea,Field,Actions,TableScroll,Dialog,Menu,Popover,Tooltip,Checkbox,Switch,Tabs} from '../src';
import '../../design-tokens/generated/tokens.css';
import '../src/styles.css';
function App(){
 const [open,setOpen]=useState(false),[checked,setChecked]=useState(false),[enabled,setEnabled]=useState(false),[tab,setTab]=useState('a'),[action,setAction]=useState('none');
 return <main style={{padding:16,display:'grid',gap:16,color:'var(--ovg-ui-ink)',background:'var(--ovg-ui-surface)'}}>
 <Actions><Button onClick={()=>setOpen(true)}>Open dialog</Button><Menu label="Actions menu" items={[{id:'a',label:'Run command',onSelect:()=>setAction('run')},{id:'b',label:'Forbidden command',disabled:true,onSelect:()=>setAction('forbidden')}]}/><Popover label="Options" title="Options panel"><Button>Option A</Button></Popover><Tooltip label="Map information">Help</Tooltip><Button onClick={()=>document.documentElement.dataset.ovgTheme='dark'}>Dark</Button></Actions>
 <output aria-label="Last action">{action}</output>
 <Checkbox label="Select layer" checked={checked} onChange={setChecked}/><Switch label="Show layer" checked={enabled} onChange={setEnabled}/>
 <Tabs label="Workspace tabs" value={tab} onChange={setTab} items={[{id:'a',label:'Map',content:<p>Map panel</p>},{id:'b',label:'Attributes',content:<p>Attribute panel</p>}]}/>
 <Dialog open={open} onOpenChange={setOpen} title="Layer properties" description="Edit layer" footer={<Button variant="primary" onClick={()=>setOpen(false)}>Save</Button>}><div style={{display:'grid',gap:12}}>
 <Field label="Layer name" hint="Required" error="Invalid name">{attrs=><Input {...attrs}/>}</Field>
 <Field label="Geometry">{attrs=><Select {...attrs}><option>Point</option><option>Polygon</option></Select>}</Field>
 <Field label="Notes">{attrs=><Textarea {...attrs}/>}</Field>
 <TableScroll><table style={{minWidth:900}}><tbody><tr><td>Wide attribute data</td></tr></tbody></table></TableScroll>
 </div></Dialog></main>;
}
createRoot(document.getElementById('root')!).render(<App/>);
