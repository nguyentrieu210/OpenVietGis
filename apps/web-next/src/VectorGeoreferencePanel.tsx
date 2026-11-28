import {useState} from "react";
import {georeferenceVector,VECTOR_GEOREF_LIMITS,type VectorGcp} from "./vector-georeference";
const blank=()=>["","","",""];
export function VectorGeoreferencePanel(){
 const [file,setFile]=useState<File|null>(null),[points,setPoints]=useState<string[][]>(()=>[blank(),blank(),blank()]),[result,setResult]=useState<ReturnType<typeof georeferenceVector>|null>(null),[error,setError]=useState(""),[busy,setBusy]=useState(false),[accepted,setAccepted]=useState(false);
 const reset=()=>{setResult(null);setAccepted(false);setError("")};
 const run=async()=>{if(!file||busy)return;reset();setBusy(true);try{
  if(!/\.(geojson|json)$/i.test(file.name)||file.size<=0||file.size>VECTOR_GEOREF_LIMITS.bytes)throw Error("GeoJSON tối đa 8 MiB.");
  const gcps=points.map(p=>p.map(s=>s.trim()?Number(s):NaN) as VectorGcp);
  const value=georeferenceVector(JSON.parse(await file.text()),gcps);setResult(value);
 }catch(e){setError(e instanceof Error?e.message:String(e))}finally{setBusy(false)}};
 const download=()=>{if(!accepted||!result)return;const url=URL.createObjectURL(new Blob([JSON.stringify(result.collection)],{type:"application/geo+json"}));const a=document.createElement("a");a.href=url;a.download="vector-georeferenced-wgs84.geojson";a.click();setTimeout(()=>URL.revokeObjectURL(url),1000)};
 return <section aria-label="Định vị vector"><h3>Định vị vector bằng GCP</h3><p>Affine 2D: tọa độ XY nguồn chưa định vị → kinh/vĩ độ WGS84. Không phải chuyển CRS; không ghi vào dự án. Chỉ XY, không kiểm tra topology toàn bộ.</p>
 <fieldset disabled={busy}><label>GeoJSON nguồn<input type="file" accept=".json,.geojson" onChange={e=>{reset();setFile(e.target.files?.[0]??null)}}/></label>
 {points.map((p,i)=><fieldset key={i}><legend>GCP {i+1}</legend>{p.map((v,j)=><label key={j}>{["X nguồn","Y nguồn","Kinh độ đích","Vĩ độ đích"][j]}<input inputMode="decimal" value={v} onChange={e=>{reset();setPoints(old=>old.map((row,k)=>k===i?row.map((x,l)=>l===j?e.target.value:x):row))}}/></label>)}{points.length>3&&<button type="button" onClick={()=>{reset();setPoints(old=>old.filter((_,j)=>j!==i))}}>Xóa GCP</button>}</fieldset>)}
 <button type="button" disabled={points.length>=32} onClick={()=>{reset();setPoints(old=>[...old,blank()])}}>Thêm GCP</button><button type="button" disabled={!file} onClick={()=>void run()}>Tính affine và kiểm tra dữ liệu</button></fieldset>
 {busy&&<p role="status">Đang xử lý vector…</p>}{error&&<p role="alert">{error}</p>}{result&&<><p>{result.collection.features.length} đối tượng · {result.vertices} đỉnh · RMS {result.rms.toPrecision(6)} độ.</p><ul>{result.residuals.map((r,i)=><li key={i}>GCP {i+1}: {r.toPrecision(6)} độ</li>)}</ul><p>Đối chiếu GCP độc lập và kiểm tra topology trước khi nhập dữ liệu. Ba GCP không đủ đánh giá độ chính xác ngoài mẫu.</p><label><input type="checkbox" checked={accepted} onChange={e=>setAccepted(e.target.checked)}/> Tôi đã kiểm tra kết quả.</label><button type="button" disabled={!accepted} onClick={download}>Tải GeoJSON WGS84</button></>}
 </section>
}
