import {useLayoutEffect,useState,type RefObject} from 'react';
import {useVirtualizer} from '@tanstack/react-virtual';
export type TableWindow={start:number;end:number;top:number;bottom:number};
// Shared row window only; existing GIS table still owns paging, ACL, revisions,
// selection, columns and row markup. Fixed-height rows use the host's CSS contract.
export function useTableWindow(scroll:RefObject<HTMLDivElement|null>,count:number,active:boolean,rowHeight=30,overscan=5):TableWindow{
 const [header,setHeader]=useState(0);
 useLayoutEffect(()=>{
  if(!active||!scroll.current)return;
  const el=scroll.current,head=el.querySelector('thead');
  const update=()=>setHeader(head?.offsetHeight??0);
  const observer=new ResizeObserver(update);observer.observe(el);if(head)observer.observe(head);update();
  return()=>observer.disconnect();
 },[active,scroll]);
 const virtual=useVirtualizer({count:active?count:0,getScrollElement:()=>active?scroll.current:null,estimateSize:()=>rowHeight,overscan,scrollMargin:header,initialRect:{width:0,height:300},enabled:active});
 const items=virtual.getVirtualItems();
 const start=items[0]?.index??0,end=items.length?items[items.length-1]!.index+1:Math.min(count,Math.ceil(300/rowHeight)+overscan);
 return {start,end,top:start*rowHeight,bottom:Math.max(0,count-end)*rowHeight};
}