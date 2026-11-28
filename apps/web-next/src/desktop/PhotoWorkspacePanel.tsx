import {useEffect,useRef,useState} from "react";
import {desktop,type PhotoInventory,type PhotoDataset,type PhotoProgress} from "./transport";

export default function PhotoWorkspacePanel({handle}:{handle:string}){
 const [inventory,setInventory]=useState<PhotoInventory|null>(null);
 const [busy,setBusy]=useState(false),[error,setError]=useState("");
 const alive=useRef(true),locked=useRef(false);
 const [dataset,setDataset]=useState<PhotoDataset|null>(null),[progress,setProgress]=useState<PhotoProgress|null>(null);
 const request=useRef<string|null>(null);
 useEffect(()=>{alive.current=true;void desktop.photoDataset(handle).then(d=>{if(alive.current)setDataset(d)}).catch(e=>{if(alive.current)setError(String(e))});return()=>{alive.current=false;if(request.current)void desktop.photoCancel(handle,request.current).catch(()=>{})}},[handle]); 
 const stage=async()=>{
  if(locked.current||dataset)return;
  locked.current=true;setBusy(true);setError("");setProgress(null);
  const id=crypto.randomUUID();request.current=id;
  const timer=setInterval(()=>{void desktop.photoStatus(handle,id).then(p=>{if(alive.current&&request.current===id&&p)setProgress(p)}).catch(()=>{})},250);
  try{const d=await desktop.photoStage(handle,id);if(alive.current&&d)setDataset(d);}
  catch(e){if(alive.current)setError(String(e));}
  finally{clearInterval(timer);request.current=null;locked.current=false;if(alive.current){setBusy(false);setProgress(null);}}
 };
 const inspect=async()=>{
  if(locked.current)return;locked.current=true;setBusy(true);setError("");
  try{const result=await desktop.photoInspect(handle);if(alive.current&&result)setInventory(result);}
  catch(e){if(alive.current)setError(String(e));}
  finally{locked.current=false;if(alive.current)setBusy(false);}
 };
 return <details>
  <summary>Ghép ảnh flycam · Photogrammetry</summary>
  <p>Kiểm tra ảnh JPEG cục bộ trước khi xử lý. Không tải ảnh lên mạng, không sửa ảnh gốc.</p>
  <button disabled={busy} onClick={()=>void inspect()}>{busy?"Đang kiểm tra…":"Chọn ảnh flycam"}</button>
  <button disabled={busy||!!dataset} onClick={()=>void stage()}>Lưu bộ ảnh vào dự án (tối đa 8 GiB)</button>
  {busy&&request.current&&<button onClick={()=>{if(request.current)void desktop.photoCancel(handle,request.current).catch(e=>setError(String(e)) )}}>Hủy nhập ảnh</button>}
  {progress&&<div role="status"><progress max={Math.max(1,progress.total_bytes)} value={progress.copied}/><span>{progress.completed}/{progress.total_files} ảnh · {(progress.copied/1024**2).toFixed(1)} MiB</span></div>}
  {dataset&&<p>Đã lưu {dataset.count} ảnh · {(dataset.bytes/1024**2).toFixed(1)} MiB · SHA-256 {dataset.manifest_sha256.slice(0,12)}…</p>}
  {inventory&&<dl>
   <dt>Số ảnh</dt><dd>{inventory.count.toLocaleString("vi-VN")}</dd>
   <dt>Dung lượng</dt><dd>{(inventory.bytes/1024**2).toFixed(1)} MiB</dd>
   <dt>Tổng pixel</dt><dd>{(inventory.pixels/1e6).toFixed(1)} MP</dd>
   <dt>Kích thước</dt><dd>{inventory.min_width}–{inventory.max_width} × {inventory.min_height}–{inventory.max_height} px</dd>
   <dt>Ảnh có GPS</dt><dd>{inventory.gps_count}/{inventory.count}</dd>
   <dt>EXIF cần kiểm tra</dt><dd>{inventory.metadata_errors}</dd>
   <dt>Hướng ảnh khác mặc định</dt><dd>{inventory.non_default_orientation}</dd>
  </dl>}
  <p role="status">Kiểm tra header, chưa xác nhận giải mã toàn bộ ảnh. GPS không xác nhận RTK. Render và GPU chưa được nghiệm thu nên chưa bật.</p>
  {error&&<p role="alert">{error}</p>}
 </details>;
}
