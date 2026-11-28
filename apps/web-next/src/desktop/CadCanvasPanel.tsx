import {useEffect,useMemo,useRef,useState} from "react";
import {nativeCad,type CadCanvasFeature,type PersistentAsset} from "./transport";

type XY=[number,number];
const points=(g:CadCanvasFeature["geometry"]):XY[]=>g.type==="Point"?[g.coordinates as XY]:g.coordinates as XY[];
export default function CadCanvasPanel({handle,asset,revision,onChanged}:{handle:string;asset:PersistentAsset;revision:string;onChanged:()=>Promise<void>}){
 const [data,setData]=useState<Awaited<ReturnType<typeof nativeCad.canvas>>>(),[selection,setSelection]=useState(""),[draft,setDraft]=useState<XY[]>([]),[vertex,setVertex]=useState(0),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const svg=useRef<SVGSVGElement>(null),alive=useRef(true),lock=useRef(false);
 useEffect(()=>{alive.current=true;return()=>{alive.current=false}},[]);
 useEffect(()=>{setData(undefined);setSelection("");setDraft([]);setError("")},[asset.id,revision]);
 const selected=data?.features.find(f=>f.properties.handle===selection);
 const bounds=useMemo(()=>{
  const p=data?.features.flatMap(f=>points(f.geometry))??[];
  const xs=p.map(v=>v[0]),ys=p.map(v=>v[1]);
  if(!p.length)return {x:0,y:0,s:1};
  const minX=Math.min(...xs),maxX=Math.max(...xs),minY=Math.min(...ys),maxY=Math.max(...ys);
  return {x:(minX+maxX)/2,y:(minY+maxY)/2,s:Math.min(560/Math.max(maxX-minX,1e-6),340/Math.max(maxY-minY,1e-6))};
 },[data]);
 const screen=(p:XY):XY=>[300+(p[0]-bounds.x)*bounds.s,190-(p[1]-bounds.y)*bounds.s];
 const choose=(f:CadCanvasFeature)=>{setSelection(f.properties.handle);setDraft(points(f.geometry).map(p=>[p[0],p[1]]));setVertex(0)};
 const run=async(fn:()=>Promise<void>)=>{if(lock.current)return;lock.current=true;setBusy(true);setError("");try{await fn()}catch(e){if(alive.current)setError(String(e))}finally{lock.current=false;if(alive.current)setBusy(false)}};
 const load=()=>run(async()=>{if(!window.confirm("Xác nhận tài liệu DXF đã có tọa độ WGS84 XY. Canvas chỉ hỗ trợ POINT/LINE, tối đa 512 entity."))return;const r=await nativeCad.canvas(handle,asset.id,revision);if(alive.current)setData(r)});
 const commit=()=>run(async()=>{
  if(!selected||!data||draft.some(p=>p.some(n=>!Number.isFinite(n))))return;
  if(!window.confirm("Tạo phiên bản DXF mới với tọa độ đã chỉnh? Bản nguồn không thay đổi."))return;
  const geometry:CadCanvasFeature["geometry"]=selected.geometry.type==="Point"?{type:"Point",coordinates:draft[0]}:{type:"LineString",coordinates:draft};
  await nativeCad.canvasCommit(handle,asset.id,revision,data.sha256,selection,geometry);if(alive.current){setData(undefined);setSelection("");setDraft([]);await onChanged()}
 });
 return <details><summary>Canvas CAD/GIS (DXF XY)</summary>
 <button disabled={busy} onClick={()=>void load()}>Mở canvas WGS84</button>
 {data&&<>
 <svg ref={svg} viewBox="0 0 600 380" role="img" aria-label="DXF coordinate editor" style={{width:"100%",background:"#f4f7f7",touchAction:"none"}} onClick={e=>{
  if(!selected||busy||!svg.current)return;const matrix=svg.current.getScreenCTM();if(!matrix)return;
  const p=new DOMPoint(e.clientX,e.clientY).matrixTransform(matrix.inverse());
  const xy:XY=[bounds.x+(p.x-300)/bounds.s,bounds.y-(p.y-190)/bounds.s];
  if(xy[0]<-180||xy[0]>180||xy[1]<-90||xy[1]>90)return;
  setDraft(d=>d.map((old,i)=>i===vertex?xy:old));
 }}>
 {data.features.map(f=>{const selected=f.properties.handle===selection;const p=(selected?draft:points(f.geometry)).map(screen);const color=selected?"#c65d0c":"#216a76";return <g key={f.properties.handle} onClick={e=>{e.stopPropagation();if(!busy)choose(f)}} style={{cursor:"pointer"}}>{p.length===1?<circle cx={p[0][0]} cy={p[0][1]} r={5} fill={color}/>:<><line x1={p[0][0]} y1={p[0][1]} x2={p[1][0]} y2={p[1][1]} stroke="transparent" strokeWidth={16}/><line x1={p[0][0]} y1={p[0][1]} x2={p[1][0]} y2={p[1][1]} stroke={color} strokeWidth={3}/></>}</g>})}
 </svg>
 <select aria-label="CAD entity" value={selection} disabled={busy} onChange={e=>{const f=data.features.find(x=>x.properties.handle===e.target.value);if(f)choose(f);else{setSelection("");setDraft([])}}}><option value="">Chọn entity</option>{data.features.map(f=><option key={f.properties.handle} value={f.properties.handle}>{f.properties.handle} · {f.properties.layer} · {f.properties.entity}</option>)}</select>
 {selected&&<div><p>Chọn đỉnh rồi chạm canvas hoặc nhập tọa độ.</p>{draft.map((p,i)=><div key={i}><label><input type="radio" checked={vertex===i} onChange={()=>setVertex(i)} disabled={busy}/>Đỉnh {i+1}</label>{p.map((n,j)=><input key={j} aria-label={j===0?"X longitude":"Y latitude"} type="number" step="any" value={Number.isFinite(n)?n:""} disabled={busy} onChange={e=>{const value=e.target.value===""?NaN:Number(e.target.value);setDraft(d=>d.map((v,k)=>k===i?(j===0?[value,v[1]]:[v[0],value]):v))}}/>)}</div>)}<button disabled={busy} onClick={()=>void commit()}>Lưu phiên bản CAD mới</button><button disabled={busy} onClick={()=>choose(selected)}>Hoàn tác draft</button></div>}
 </>}
 <p role="status">{error}</p>
 </details>
}
