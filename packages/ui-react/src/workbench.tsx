import {useEffect,useRef,useState,type ReactNode,type FunctionComponent} from 'react';
import {DockviewReact,type DockviewApi,type DockviewReadyEvent,type IDockviewPanelProps,type IDockviewPanelHeaderProps,type SerializedDockview} from 'dockview-react';
import 'dockview-react/dist/styles/dockview.css';
import './workbench.css';
import {Button} from './primitives';

export type PanelSpec={id:string;title:string;render:FunctionComponent<IDockviewPanelProps>;persistent?:boolean};
export const workbenchWindowCapabilities=Object.freeze({sameWindowFloating:true,browserPopout:false,nativeWindow:false});
export type Preset='qgis'|'map'|'analysis';
export type Scope={account:string;project:string;workspace:string};
type StorageLike=Pick<Storage,'getItem'|'setItem'|'removeItem'>;
const keyId=/^[a-zA-Z0-9][a-zA-Z0-9_.:-]{0,95}$/;
const object=(x:unknown):x is Record<string,unknown>=>!!x&&typeof x==='object'&&!Array.isArray(x);
const finite=(x:unknown)=>typeof x==='number'&&Number.isFinite(x)&&x>=0&&x<=100000;
export function layoutKey(scope:Scope){for(const id of Object.values(scope))if(!keyId.test(id))throw Error('Invalid scope');return 'ovg:workbench:v1:'+JSON.stringify([scope.account,scope.project,scope.workspace])}
export function validateRegistry(specs:readonly PanelSpec[]){
 const ids=new Set<string>();for(const s of specs){if(!keyId.test(s.id)||ids.has(s.id)||!s.title.trim()||s.title.length>160)throw Error('Invalid panel registry');ids.add(s.id)}
 if(specs.length>64||!ids.has('map')||!specs.find(s=>s.id==='map')?.persistent)throw Error('Required map panel missing');
}
export function decodeLayout(raw:string|null,specs:readonly PanelSpec[]):SerializedDockview|null{
 if(!raw||raw.length>128000)return null;
 try{
  const e:unknown=JSON.parse(raw);if(!object(e)||e.version!==1||!object(e.layout))return null;
  const l=e.layout;if(!object(l.grid)||!object(l.panels)||!finite(l.grid.width)||!finite(l.grid.height)||!['HORIZONTAL','VERTICAL',0,1].includes(l.grid.orientation as never))return null;
  // Cross-window/edge-group payloads are not accepted by this same-window adapter.
  if(l.popoutGroups!==undefined&&(!Array.isArray(l.popoutGroups)||l.popoutGroups.length))return null;
  if(l.edgeGroups!==undefined)return null;
  const registry=new Map(specs.map(s=>[s.id,s]));const panels=Object.keys(l.panels);if(panels.length>64)return null;
  for(const id of panels){const p=l.panels[id],s=registry.get(id);if(!s||!object(p)||p.id!==id||p.contentComponent!==id||(p.tabComponent!==undefined&&p.tabComponent!=='props.defaultTabComponent')||p.title!==s.title||p.renderer!=='always'||(p.params!==undefined&&(!object(p.params)||Object.keys(p.params).length)))return null;}
  if(specs.some(s=>s.persistent&&!panels.includes(s.id)))return null;
  const seen=new Set<string>(),groups=new Set<string>();let nodes=0;
  const group=(g:unknown):boolean=>{if(!object(g)||typeof g.id!=='string'||!keyId.test(g.id)||groups.has(g.id)||!Array.isArray(g.views)||g.views.length>64||g.tabGroups!==undefined)return false;groups.add(g.id);
   if(g.activeView!==undefined&&!g.views.includes(g.activeView))return false;
   for(const id of g.views){if(typeof id!=='string'||!panels.includes(id)||seen.has(id))return false;seen.add(id)}return true;};
  const node=(n:unknown,depth=0):boolean=>{if(++nodes>128||depth>16||!object(n)||(n.size!==undefined&&!finite(n.size)))return false;
   return n.type==='leaf'?group(n.data):n.type==='branch'&&Array.isArray(n.data)&&n.data.every(x=>node(x,depth+1));};
  if(!node(l.grid.root))return null;
  if(l.floatingGroups!==undefined){if(!Array.isArray(l.floatingGroups)||l.floatingGroups.length>32)return null;for(const f of l.floatingGroups){if(!object(f)||!object(f.position)||Object.values(f.position).some(x=>typeof x==='number'&&!finite(Math.abs(x))))return null;
   if(f.grid!==undefined||!group(f.data))return null;}}
  if(seen.size!==panels.length||(l.activeGroup!==undefined&&(typeof l.activeGroup!=='string'||!groups.has(l.activeGroup))))return null;
  return l as unknown as SerializedDockview;
 }catch{return null}
}
export function readLayout(storage:StorageLike,key:string,specs:readonly PanelSpec[]){try{return decodeLayout(storage.getItem(key),specs)}catch{return null}}
export function saveLayout(storage:StorageLike,key:string,layout:SerializedDockview,specs:readonly PanelSpec[]){try{const raw=JSON.stringify({version:1,layout});if(!decodeLayout(raw,specs))return false;storage.setItem(key,raw);return true}catch{return false}}
export function initPreset(api:DockviewApi,specs:readonly PanelSpec[],preset:Preset){
 api.clear();
 const add=(id:string,position?:{referencePanel:string;direction:'left'|'right'|'below'|'within'},initialWidth?:number,initialHeight?:number)=>{const s=specs.find(x=>x.id===id);if(s)api.addPanel({id:s.id,component:s.id,title:s.title,renderer:'always',position,initialWidth,initialHeight,minimumWidth:120,minimumHeight:80})};
 add('map');
 if(preset!=='map'){add('layers',{referencePanel:'map',direction:'left'},240);add('inspector',{referencePanel:'map',direction:'right'},260)}
 if(preset==='analysis')add('attributes',{referencePanel:'map',direction:'below'},undefined,220);
 for(const s of specs)if(s.persistent&&!api.getPanel(s.id))add(s.id,{referencePanel:'map',direction:'within'});
 api.getPanel('map')?.api.setActive();
}
export function Workbench({scope,panels,preset='qgis',children,onReady,onPersistence,canReset,locked=false,managed=false,visiblePanels}:{scope:Scope;panels:readonly PanelSpec[];preset?:Preset;children?:ReactNode;onReady?:(api:DockviewApi)=>void;onPersistence?:(saved:boolean)=>void;canReset?:()=>boolean;locked?:boolean;managed?:boolean;visiblePanels?:readonly string[]}){
 const key=layoutKey(scope);
 // Changing the account/project/workspace remounts the isolated engine; never share old layout.
 return <ScopedWorkbench key={key} storageKey={key} panels={panels} preset={preset} onReady={onReady} onPersistence={onPersistence} canReset={canReset} locked={locked} managed={managed} visiblePanels={visiblePanels}>{children}</ScopedWorkbench>
}
function ScopedWorkbench({storageKey,panels,preset,children,onReady,onPersistence,canReset,locked=false,managed=false,visiblePanels}:{storageKey:string;panels:readonly PanelSpec[];preset:Preset;children?:ReactNode;onReady?:(api:DockviewApi)=>void;onPersistence?:(saved:boolean)=>void;canReset?:()=>boolean;locked?:boolean;managed?:boolean;visiblePanels?:readonly string[]}){
 validateRegistry(panels);
 const [api,setApi]=useState<DockviewApi|null>(null),[selected,setSelected]=useState('map');const disposables=useRef<{dispose:()=>void}[]>([]),timer=useRef<ReturnType<typeof setTimeout>|undefined>(undefined);
 const live=useRef({panels,onReady,onPersistence});live.current={panels,onReady,onPersistence};
 const [registry]=useState(()=>Object.fromEntries(panels.map(s=>[s.id,s.render])));
 const registryIds=Object.keys(registry).sort().join('|');if(registryIds!==panels.map(s=>s.id).sort().join('|'))throw Error('Remount workbench for registry changes');
 const persist=(a:DockviewApi)=>{try{live.current.onPersistence?.(saveLayout(localStorage,storageKey,a.toJSON(),live.current.panels))}catch{live.current.onPersistence?.(false)}};
 const ready=({api:a}:DockviewReadyEvent)=>{for(const d of disposables.current)d.dispose();disposables.current=[];
  let restored=false;try{const layout=readLayout(localStorage,storageKey,panels);if(layout){a.fromJSON(layout);restored=true}}catch{/* fallback */}
  if(!restored)initPreset(a,panels,preset);
  disposables.current.push(a.onDidLayoutChange(()=>{clearTimeout(timer.current);timer.current=setTimeout(()=>persist(a),120)}));
  setApi(a);live.current.onReady?.(a);persist(a);
 };
 useEffect(()=>()=>{clearTimeout(timer.current);for(const d of disposables.current)d.dispose();disposables.current=[]},[]);
 const tab=({api:panelApi}:IDockviewPanelHeaderProps)=><span className="ovg-workbench-tab">{panelApi.title}<button type="button" hidden={managed||panels.some(s=>s.id===panelApi.id&&s.persistent)} disabled={locked} aria-label={'Close '+panelApi.title} onClick={e=>{e.stopPropagation();if(!locked&&!managed)panelApi.close()}}>×</button></span>;
 const options=managed&&visiblePanels?panels.filter(p=>visiblePanels.includes(p.id)):panels;
 const activeSelection=options.some(p=>p.id===selected)?selected:'map';
 const find=()=>api?.getPanel(activeSelection);
 const open=()=>{if(!api||locked)return;const existing=find(),s=panels.find(p=>p.id===activeSelection);if(existing){existing.api.setActive();return}if(s&&!managed)api.addPanel({id:s.id,component:s.id,title:s.title,renderer:'always',position:{referencePanel:'map',direction:'within'}})};
 const move=(direction:'left'|'right'|'bottom'|'center')=>{const p=find(),map=api?.getPanel('map');if(!locked&&p&&map&&p.id!==map.id)p.api.moveTo({group:map.group,position:direction})};
 const float=()=>{const p=find();if(!locked&&p&&api&&p.id!=='map')api.addFloatingGroup(p,{width:300,height:220,x:24,y:24})};
 return <section className="ovg-workbench"><div className="ovg-workbench-tools">
 <select aria-label="Panel" value={activeSelection} onChange={e=>setSelected(e.target.value)}>{options.map(p=><option key={p.id} value={p.id}>{p.title}</option>)}</select>
 <Button disabled={locked} onClick={open}>Open panel</Button><Button disabled={locked} onClick={()=>move('left')}>Dock left</Button><Button disabled={locked} onClick={()=>move('right')}>Dock right</Button><Button disabled={locked} onClick={()=>move('bottom')}>Dock bottom</Button><Button disabled={locked} onClick={()=>move('center')}>Dock tab</Button><Button disabled={locked} onClick={float}>Float panel</Button>
 <Button disabled={locked} onClick={()=>{const p=find();if(p)p.api.isMaximized()?p.api.exitMaximized():p.api.maximize()}}>Maximize / restore</Button>
 <Button disabled={locked||!canReset} onClick={()=>{if(!locked&&api&&canReset?.()===true)initPreset(api,panels,preset)}}>Reset layout</Button>{children}</div>
 <div className="ovg-workbench-body"><DockviewReact components={registry} defaultTabComponent={tab} onReady={ready} theme={{name:'ovg',className:'dockview-theme-ovg'}} floatingGroupBounds="boundedWithinViewport" getTabContextMenuItems={()=>[]} disableDnd={locked} /></div></section>
}