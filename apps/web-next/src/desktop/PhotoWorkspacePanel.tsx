import {useEffect,useRef,useState} from "react";
import {desktop,type PhotoInventory} from "./transport";

export default function PhotoWorkspacePanel({handle}:{handle:string}){
 const [inventory,setInventory]=useState<PhotoInventory|null>(null);
 const [busy,setBusy]=useState(false),[error,setError]=useState("");
 const alive=useRef(true),locked=useRef(false);
 useEffect(()=>{alive.current=true;return()=>{alive.current=false}},[]);
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
