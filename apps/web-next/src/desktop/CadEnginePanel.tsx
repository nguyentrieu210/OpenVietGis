import {useEffect,useRef,useState} from "react";
import {nativeCad,type EngineEdit} from "./transport";
type Entity={handle:string;type:string;layer:string};
export default function CadEnginePanel({handle,asset,revision,onChanged,disabled=false}:{handle:string;asset:string;revision:string;onChanged:()=>Promise<void>;disabled?:boolean}){
 const [entities,setEntities]=useState<Entity[]>([]),[sha,setSha]=useState(""),[selected,setSelected]=useState(""),[mode,setMode]=useState<EngineEdit["type"]>("move"),[values,setValues]=useState(["0","0"]),[error,setError]=useState(""),[busy,setBusy]=useState(false);
 const epoch=useRef(0),lock=useRef(false),mounted=useRef(true);
 useEffect(()=>{mounted.current=true;return()=>{mounted.current=false}},[]);
 useEffect(()=>{epoch.current++;setEntities([]);setSha("");setSelected("");setError("");return()=>{epoch.current++}},[handle,asset,revision]);
 const run=async(f:(g:number)=>Promise<void>)=>{if(lock.current||disabled)return;lock.current=true;setBusy(true);setError("");const g=epoch.current;try{await f(g)}catch(e){if(g===epoch.current)setError(String(e))}finally{lock.current=false;if(mounted.current)setBusy(false)}};
 const load=()=>run(async g=>{
  if(!window.confirm("Mở bản sao trong OpenVietCAD native headless? Chỉ sử dụng bản vẽ bạn tin cậy."))return;
  const r=await nativeCad.engineEntities(handle,asset,revision);
  if(g===epoch.current){setEntities(r.entities);setSha(r.sha256);setSelected(r.entities[0]?.handle??"")}
 });
 const labels=mode==="move"?["ΔX","ΔY"]:mode==="rotate"?["Tâm X","Tâm Y","Góc (°)"]:["Tâm X","Tâm Y","Hệ số"];
 const changeMode=(v:EngineEdit["type"])=>{setMode(v);setValues(v==="move"?["0","0"]:v==="rotate"?["0","0","90"]:["0","0","2"])};
 const commit=()=>run(async g=>{
  if(!selected||!sha||values.some(v=>!v.trim()))return;
  const n=values.map(Number);if(n.some(v=>!Number.isFinite(v)))return;
  const edit:EngineEdit=mode==="move"?{type:mode,dx:n[0],dy:n[1]}:mode==="rotate"?{type:mode,x:n[0],y:n[1],angle:n[2]}:{type:mode,x:n[0],y:n[1],factor:n[2]};
  if(!window.confirm("Chạy phép biến đổi trong OpenVietCAD Engine và chỉ nhận bản lưu đã kiểm tra? File gốc không đổi."))return;
  await nativeCad.engineTransform(handle,asset,revision,sha,selected,edit);
  if(g===epoch.current){setEntities([]);setSha("");setSelected("")}
  if(mounted.current)await onChanged();
 });
 return <details><summary>OpenVietCAD Engine – thao tác native</summary>
  <p>DWG/DXF: di chuyển, xoay và scale entity bằng engine gốc. Tối đa 512 entity; từ chối thay đổi phụ thuộc ngoài entity được chọn. Bản lưu phải vượt qua save_verified.</p>
  <button disabled={busy||disabled} onClick={()=>void load()}>Đọc entity từ engine</button>
  {sha&&<><label>Entity <select value={selected} disabled={busy||disabled} onChange={e=>setSelected(e.target.value)}>{entities.map(v=><option key={v.handle} value={v.handle}>{v.handle} · {v.type} · {v.layer}</option>)}</select></label>
  <label>Thao tác <select value={mode} disabled={busy||disabled} onChange={e=>changeMode(e.target.value as EngineEdit["type"])}><option value="move">Di chuyển</option><option value="rotate">Xoay</option><option value="scale">Scale đều</option></select></label>
  {labels.map((l,i)=><label key={l}>{l}<input type="number" step="any" value={values[i]??""} disabled={busy||disabled} onChange={e=>setValues(old=>old.map((v,j)=>j===i?e.target.value:v))}/></label>)}
  <button disabled={busy||disabled||!selected} onClick={()=>void commit()}>Tạo phiên bản CAD đã xác minh</button></>}
  {busy&&<p role="status">Đang chạy engine native…</p>}{error&&<p role="alert">{error}</p>}
 </details>
}
