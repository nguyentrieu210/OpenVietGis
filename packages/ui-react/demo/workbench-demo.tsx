import {useEffect,useRef,useState} from 'react';
import {Workbench,type PanelSpec} from '../src/workbench';
import type {IDockviewPanelProps} from 'dockview-react';
let mounts=0;
function MapPanel({api}:IDockviewPanelProps){
 const [count,setCount]=useState(0),[mount]=useState(()=>++mounts);const canvas=useRef<HTMLCanvasElement>(null);
 useEffect(()=>{const el=canvas.current!;const resize=()=>{const r=el.getBoundingClientRect();if(r.width&&r.height){el.width=r.width;el.height=r.height;const c=el.getContext('2d');if(c){c.fillStyle='#1565a9';c.fillRect(0,0,el.width,el.height)}}};const obs=new ResizeObserver(resize);obs.observe(el);resize();return()=>obs.disconnect()},[]);
 return <div data-testid="map-panel" data-mount={mount} style={{height:'100%',display:'flex',flexDirection:'column'}}><button onClick={()=>setCount(c=>c+1)}>Edit {count}</button><button onClick={()=>api.setActive()}>Activate map</button><canvas ref={canvas} style={{flex:1,minHeight:0,width:'100%'}}/></div>;
}
const Simple=({api}:IDockviewPanelProps)=><div data-testid={api.id+'-panel'}>{api.title} content</div>;
const panels:PanelSpec[]=[{id:'map',title:'Map',render:MapPanel,persistent:true},{id:'layers',title:'Layers',render:Simple},{id:'inspector',title:'Inspector',render:Simple},{id:'attributes',title:'Attributes',render:Simple}];
export function WorkbenchDemo(){
 const [account,setAccount]=useState('a'),[saved,setSaved]=useState(false);
 return <div style={{height:'98vh'}}><Workbench scope={{account,project:'p1',workspace:'gis'}} panels={panels} canReset={()=>true} onPersistence={setSaved}><button onClick={()=>setAccount(a=>a==='a'?'b':'a')}>Switch account</button><output data-testid="saved">{saved?'saved':'not saved'}</output></Workbench></div>;
}