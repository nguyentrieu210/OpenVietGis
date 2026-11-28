import {useEffect,useMemo,useRef,useState} from "react";
import {nativeCad,type CadStroke,type CadEdit} from "./transport";
export default function CadPreviewPanel({handle,asset,revision,onChanged}:{handle:string;asset:string;revision:string;onChanged:()=>Promise<void>}){
 const [data,setData]=useState<CadStroke[]>([]),[busy,setBusy]=useState(false),[error,setError]=useState(""),[selected,setSelected]=useState(""),[hidden,setHidden]=useState<Set<string>>(new Set());
 const generation=useRef(0),lock=useRef(false);
 const [sha,setSha]=useState(""),[mode,setMode]=useState<CadEdit["type"]>("translate"),[values,setValues]=useState<string[]>(["0","0"]);
 const fields:Record<CadEdit["type"],string[]>={translate:["ΔX","ΔY"],vertex:["Path (0-based)","Vertex (0-based)","X","Y"],radius:["Radius"],arc_angles:["Start angle","End angle"],block_transform:["Scale X","Scale Y","Rotation (°)"]};
 const changeMode=(m:CadEdit["type"])=>{setMode(m);setValues(m==="block_transform"?["1","1","0"]:fields[m].map(()=>"0"))};
 useEffect(()=>{generation.current++;setBusy(false);setData([]);setSha("");setSelected("");changeMode("translate");setHidden(new Set());setError("");return()=>{generation.current++}},[handle,asset,revision]);
 const layers=useMemo(()=>[...new Set(data.map(s=>s.layer))].sort(),[data]);
 const bounds=useMemo(()=>{
  let minX=Infinity,minY=Infinity,maxX=-Infinity,maxY=-Infinity;
  for(const s of data)for(const path of s.paths)for(const [x,y] of path){minX=Math.min(minX,x);minY=Math.min(minY,y);maxX=Math.max(maxX,x);maxY=Math.max(maxY,y)}
  if(!Number.isFinite(minX))return {x:0,y:0,s:1};
  return {x:(minX+maxX)/2,y:(minY+maxY)/2,s:Math.min(560/Math.max(maxX-minX,1e-9),340/Math.max(maxY-minY,1e-9))};
 },[data]);
 const point=([x,y]:[number,number])=>[300+(x-bounds.x)*bounds.s,190-(y-bounds.y)*bounds.s];
 const load=async()=>{if(busy||lock.current)return;const g=++generation.current;setBusy(true);setError("");try{const r=await nativeCad.preview(handle,asset,revision);if(g===generation.current){setData(r.strokes);setSha(r.sha256)}}catch(e){if(g===generation.current){setData([]);setError(String(e))}}finally{if(g===generation.current)setBusy(false)}};
 const entity=data.find(s=>s.handle===selected);
 const modes:CadEdit["type"][]=["translate",...(entity?.kind==="LWPOLYLINE"||entity?.kind==="HATCH"?["vertex" as const]:[]),...(entity?.kind==="ARC"||entity?.kind==="CIRCLE"?["radius" as const]:[]),...(entity?.kind==="ARC"?["arc_angles" as const]:[]),...(entity?.kind==="INSERT"?["block_transform" as const]:[])];
 const move=async()=>{if(lock.current||busy||!entity||!sha||!modes.includes(mode)||values.some(v=>!v.trim()))return;const n=values.map(Number);if(n.some(v=>!Number.isFinite(v)))return;
  let edit:CadEdit;
  switch(mode){
   case "translate":edit={type:mode,dx:n[0],dy:n[1]};break;
   case "vertex":if(!Number.isSafeInteger(n[0])||!Number.isSafeInteger(n[1])||n[0]<0||n[1]<0)return;edit={type:mode,path:n[0],vertex:n[1],x:n[2],y:n[3]};break;
   case "radius":edit={type:mode,radius:n[0]};break;
   case "arc_angles":edit={type:mode,start:n[0],end:n[1]};break;
   case "block_transform":edit={type:mode,scale_x:n[0],scale_y:n[1],rotation:n[2]};break;
  }
  lock.current=true;const g=++generation.current;setBusy(true);setError("");
  try{await nativeCad.reshape(handle,asset,revision,sha,entity.handle,edit);if(g===generation.current){setData([]);setSha("");setSelected("");await onChanged()}}
  catch(e){if(g===generation.current)setError(String(e))}finally{lock.current=false;if(g===generation.current)setBusy(false)}
 };
 return <details><summary>Chỉnh sửa CAD XY mở rộng</summary><p>Toạ độ CAD gốc, chưa gán CRS. Arc/Circle được lấy mẫu để hiển thị; hatch đặc và block có giới hạn. Chỉnh sửa tọa độ gốc, bán kính, góc hoặc block transform theo kiểu entity; tạo phiên bản mới, không ghi từ đường lấy mẫu.</p>
 <button disabled={busy} onClick={()=>void load()}>Đọc DXF</button>
 {data.length>0&&<><div>{layers.map(l=><label key={l} style={{marginRight:8}}><input type="checkbox" checked={!hidden.has(l)} onChange={()=>setHidden(old=>{const n=new Set(old);if(n.has(l))n.delete(l);else n.add(l);return n})}/>{l}</label>)}</div>
 <svg viewBox="0 0 600 380" role="img" aria-label="CAD preview" style={{width:"100%",background:"#f1f5f6"}}>
 {data.filter(s=>!hidden.has(s.layer)).map(s=>{const color=selected===s.handle?"#c65d0c":"#216a76";const d=s.paths.filter(p=>p.length>1).map(p=>p.map((v,i)=>{const [x,y]=point(v);return (i?"L":"M")+x+" "+y}).join(" ")+(s.fill?" Z":"")).join(" ");
 return <g key={s.handle} onClick={()=>{if(!busy){setSelected(s.handle);changeMode("translate")}}} style={{cursor:"pointer"}}><path d={d} stroke="transparent" strokeWidth={12} fill="none"/><path d={d} stroke={color} strokeWidth={2} fill={s.fill?color:"none"} fillOpacity={0.24} fillRule="evenodd"/>{s.paths.filter(p=>p.length===1).map((p,i)=>{const [x,y]=point(p[0]);return <circle key={i} cx={x} cy={y} r={4} fill={color}/>})}</g>})}
 </svg><p>{data.length} entity {entity&&"· "+entity.handle+" · "+entity.kind+" · "+entity.layer}</p>{entity&&<div><label>Thao tác <select value={mode} disabled={busy} onChange={e=>changeMode(e.target.value as CadEdit["type"])}>{modes.map(m=><option key={m} value={m}>{({translate:"Di chuyển",vertex:"Chỉnh đỉnh",radius:"Bán kính",arc_angles:"Góc cung",block_transform:"Scale / xoay block"})[m]}</option>)}</select></label>{fields[mode].map((label,i)=><label key={label}>{label} <input type="number" step="any" value={values[i]??""} disabled={busy} onChange={e=>setValues(old=>old.map((v,j)=>i===j?e.target.value:v))}/></label>)}<button disabled={busy} onClick={()=>void move()}>Lưu phiên bản CAD mới</button></div>}</>}
 <p role="status">{error}</p></details>
}
