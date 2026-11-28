import {useMemo,useState} from 'react';
import {DataGrid,type GridRow,type GridQuery,type GridColumn} from '../src/data';
import {PluginPanel,type UniversalSession} from '../src/plugin';
import {CommandRegistry,CommandPalette,type CommandContext} from '../src/commands';
import {PreferencesPanel,applyPreferences,defaults,type Preferences} from '../src/preferences';
import '../src/preferences/styles.css';
const rows=Array.from({length:100000},(_,i):GridRow=>({id:'row-'+i,revision:'r1',values:{name:'Feature '+i,amount:i}}));
const columns:GridColumn[]=[{id:'name',label:'Name',field:{kind:'text',editable:true}},{id:'amount',label:'Amount',field:{kind:'number',editable:true,min:0}}];
export function PlatformDemo(){
 const [query,setQuery]=useState<GridQuery>({page:0,pageSize:100000,filter:'',sorting:[]}),[selection,setSelection]=useState<Record<string,boolean>>({}),[status,setStatus]=useState(''),[open,setOpen]=useState(false),[pref,setPref]=useState<Preferences>({...defaults,locale:'en'});
 const filtered=useMemo(()=>{const a=rows.filter(r=>String(r.values.name).includes(query.filter));if(query.sorting[0])a.sort((a,b)=>(Number(a.values.amount)-Number(b.values.amount))*(query.sorting[0].desc?-1:1));return a},[query.filter,query.sorting]);
 const registry=useMemo(()=>{const r=new CommandRegistry();r.register({id:'map.zoom',labels:{en:'Zoom map',vi:'Thu phóng'},group:'Map',shortcut:'ctrl+shift+z',capability:'map.read',run:()=>setStatus('zoom')});return r},[]);
 const context=():CommandContext=>({scope:'project-1',locale:pref.locale,capabilities:new Set(['map.read'])});
 const session=useMemo<UniversalSession>(()=>({scopeId:'p1',sessionId:'s1',nodes:[{id:'t',kind:'text',label:'Host read-only',children:[]},{id:'i',kind:'input',label:'Notice',binding:'text',children:[]},{id:'b',kind:'button',label:'Send notice',binding:'a',children:[]}],actions:new Set(['a']),fields:new Set(['text']),isCurrent:()=>true,invoke:async(id,values)=>({id,values})}),[]);
 return <main style={{padding:12,display:'grid',gap:12}}><button onClick={()=>setOpen(true)}>Command palette</button><CommandPalette registry={registry} context={context} open={open} onOpenChange={setOpen}/><output data-testid="platform-status">{status}</output>
 <DataGrid rows={filtered.slice(query.page*query.pageSize,(query.page+1)*query.pageSize)} columns={columns} total={filtered.length} query={query} onQuery={setQuery} selection={selection} onSelection={setSelection} onEdit={async e=>setStatus(JSON.stringify(e))}/>
 <PluginPanel session={session}/><PreferencesPanel value={pref} onChange={p=>{setPref(p);applyPreferences(document.documentElement,p)}}/></main>;
}