import {useState} from 'react';
import {LayerTree,Legend,CrsSelector,SymbolEditor,ColorRamp,CoordinateInput,SnapToolbar,PropertyEditor,type LayerNode,type SymbolStyle,type SnapConfig,type PropertyField,type CrsOption} from '../src/gis';
const symbol:SymbolStyle={color:'#1565a9',stroke:'#263c49',opacity:1,size:6};
const initial:LayerNode[]=[{kind:'group',id:'g',name:'Survey',children:[{kind:'layer',id:'a',name:'Roads',visible:true,canVisibility:true,canReorder:true,family:'line',symbol},{kind:'layer',id:'b',name:'Parcels',visible:false,canVisibility:true,canReorder:true,family:'polygon',symbol},{kind:'layer',id:'c',name:'Protected',visible:true,locked:true,canVisibility:false,canReorder:false,family:'point',symbol}]}];
const catalog:CrsOption[]=[{id:'EPSG:4326',name:'WGS84',group:'Geographic',unit:'degree',axisOrder:'xy'},{id:'EPSG:3405',name:'VN-2000 UTM 48N',group:'VN-2000',unit:'metre',axisOrder:'xy'}];
const fields:PropertyField[]=[{id:'name',label:'Feature name',kind:'text',value:'Road A',editable:true},{id:'length',label:'Length',kind:'number',value:10,editable:true,min:0,max:1000},{id:'fixed',label:'Handle',kind:'text',value:'AB12',editable:false},{id:'verified',label:'Verified',kind:'boolean',value:false,editable:true}];
export function GisDemo(){
 const [nodes,setNodes]=useState(initial),[selected,setSelected]=useState<string[]>([]),[crs,setCrs]=useState('EPSG:4326'),[style,setStyle]=useState(symbol),[snap,setSnap]=useState<SnapConfig>({enabled:true,kinds:['vertex'],tolerance:10,unit:'px',scope:'active'}),[event,setEvent]=useState(''),[revision,setRevision]=useState('r1');
 return <main style={{padding:8,display:'grid',gridTemplateColumns:'repeat(auto-fit,minmax(min(100%,280px),1fr))',gap:12}}>
 <LayerTree nodes={nodes} selected={selected} onSelect={(id,extend)=>setSelected(v=>extend?[...new Set([...v,id])]:[id])} onVisibility={changes=>setNodes(v=>v.map(g=>g.kind==='group'?{...g,children:g.children.map(n=>n.kind==='layer'?{...n,visible:changes.find(c=>c.id===n.id)?.visible??n.visible}:n)}:g))} onMove={(id,direction)=>setEvent(JSON.stringify({id,direction}))}/>
 <Legend partial items={[{id:'a',label:'Roads',family:'line',symbol:style}]}/>
 <CrsSelector options={catalog} value={crs} onChange={setCrs}/>
 <SymbolEditor family="polygon" value={style} onApply={v=>{setStyle(v);setEvent(JSON.stringify(v))}}/>
 <ColorRamp from="#000000" to="#ffffff" steps={3} onApply={v=>setEvent(JSON.stringify(v))}/>
 <CoordinateInput context={{crsId:crs,unit:crs==='EPSG:4326'?'degree':'metre',axisOrder:'xy',geographic:crs==='EPSG:4326'}} onSubmit={v=>setEvent(JSON.stringify(v))}/>
 <SnapToolbar value={snap} available={['vertex','edge']} onChange={v=>{setSnap(v);setEvent(JSON.stringify(v))}}/>
 <PropertyEditor entityId="feature-1" revision={revision} fields={fields} onApply={v=>setEvent(JSON.stringify(v))}/>
 <button onClick={()=>setRevision('r2')}>Change revision</button><output data-testid="gis-event">{event}</output>
 </main>;
}