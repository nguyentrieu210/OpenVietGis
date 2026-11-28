import {useEffect,useMemo,useRef,useState} from "react";
import {nativeCad,type CadStroke} from "./transport";
export default function CadPreviewPanel({handle,asset,revision}:{handle:string;asset:string;revision:string}){
 const [data,setData]=useState<CadStroke[]>([]),[busy,setBusy]=useState(false),[error,setError]=useState(""),[selected,setSelected]=useState(""),[hidden,setHidden]=useState<Set<string>>(new Set());
 const generation=useRef(0);
 useEffect(()=>{generation.current++;setBusy(false);setData([]);setSelected("");setHidden(new Set());setError("");return()=>{generation.current++}},[handle,asset,revision]);
 const layers=useMemo(()=>[...new Set(data.map(s=>s.layer))].sort(),[data]);
 const bounds=useMemo(()=>{
  let minX=Infinity,minY=Infinity,maxX=-Infinity,maxY=-Infinity;
  for(const s of data)for(const path of s.paths)for(const [x,y] of path){minX=Math.min(minX,x);minY=Math.min(minY,y);maxX=Math.max(maxX,x);maxY=Math.max(maxY,y)}
  if(!Number.isFinite(minX))return {x:0,y:0,s:1};
  return {x:(minX+maxX)/2,y:(minY+maxY)/2,s:Math.min(560/Math.max(maxX-minX,1e-9),340/Math.max(maxY-minY,1e-9))};
 },[data]);
 const point=([x,y]:[number,number])=>[300+(x-bounds.x)*bounds.s,190-(y-bounds.y)*bounds.s];
 const load=async()=>{if(busy)return;const g=++generation.current;setBusy(true);setError("");try{const r=await nativeCad.preview(handle,asset,revision);if(g===generation.current)setData(r.strokes)}catch(e){if(g===generation.current){setData([]);setError(String(e))}}finally{if(g===generation.current)setBusy(false)}};
 const entity=data.find(s=>s.handle===selected);
 return <details><summary>Xem trước CAD mở rộng</summary><p>Toạ độ CAD gốc, chưa gán CRS. Arc/Circle được lấy mẫu để hiển thị; hatch đặc và block có giới hạn. Không dùng hình xem trước để ghi ngược bản vẽ.</p>
 <button disabled={busy} onClick={()=>void load()}>Đọc DXF</button>
 {data.length>0&&<><div>{layers.map(l=><label key={l} style={{marginRight:8}}><input type="checkbox" checked={!hidden.has(l)} onChange={()=>setHidden(old=>{const n=new Set(old);if(n.has(l))n.delete(l);else n.add(l);return n})}/>{l}</label>)}</div>
 <svg viewBox="0 0 600 380" role="img" aria-label="CAD preview" style={{width:"100%",background:"#f1f5f6"}}>
 {data.filter(s=>!hidden.has(s.layer)).map(s=>{const color=selected===s.handle?"#c65d0c":"#216a76";const d=s.paths.filter(p=>p.length>1).map(p=>p.map((v,i)=>{const [x,y]=point(v);return (i?"L":"M")+x+" "+y}).join(" ")+(s.fill?" Z":"")).join(" ");
 return <g key={s.handle} onClick={()=>setSelected(s.handle)} style={{cursor:"pointer"}}><path d={d} stroke="transparent" strokeWidth={12} fill="none"/><path d={d} stroke={color} strokeWidth={2} fill={s.fill?color:"none"} fillOpacity={0.24} fillRule="evenodd"/>{s.paths.filter(p=>p.length===1).map((p,i)=>{const [x,y]=point(p[0]);return <circle key={i} cx={x} cy={y} r={4} fill={color}/>})}</g>})}
 </svg><p>{data.length} entity {entity&&"· "+entity.handle+" · "+entity.kind+" · "+entity.layer}</p></>}
 <p role="status">{error}</p></details>
}
