import {useRef} from 'react';
import {useVirtualizer} from '@tanstack/react-virtual';
export type TreeRow={id:string;label:string;depth:number;hasChildren:boolean};
export function VirtualTree({rows,expanded,selected,onExpand,onSelect}:{rows:readonly TreeRow[];expanded:ReadonlySet<string>;selected:string|null;onExpand:(id:string)=>void;onSelect:(id:string)=>void}){
 const scroll=useRef<HTMLDivElement>(null);
 const v=useVirtualizer({count:rows.length,getScrollElement:()=>scroll.current,estimateSize:()=>32,overscan:8,getItemKey:i=>rows[i]?.id??i});
 return <div ref={scroll} aria-label="Virtual tree" role="tree" className="ovg-grid-scroll"><div style={{height:v.getTotalSize(),position:'relative'}}>{v.getVirtualItems().map(i=>{const r=rows[i.index];return <div key={r.id} role="treeitem" aria-level={r.depth+1} aria-selected={selected===r.id} aria-expanded={r.hasChildren?expanded.has(r.id):undefined} style={{position:'absolute',top:i.start,height:32,left:0,right:0,paddingInlineStart:Math.min(32,r.depth)*12,display:'flex',alignItems:'center'}}>{r.hasChildren&&<button aria-label={(expanded.has(r.id)?'Collapse ':'Expand ')+r.label} onClick={()=>onExpand(r.id)}>{expanded.has(r.id)?'▾':'▸'}</button>}<button onClick={()=>onSelect(r.id)}>{r.label}</button></div>})}</div></div>;
}