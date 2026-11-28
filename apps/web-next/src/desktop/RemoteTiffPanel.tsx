import {useEffect,useRef,useState} from "react";
import {remoteTiff,type RemoteTiffInfo} from "./transport";
export default function RemoteTiffPanel({handle}:{handle:string}){
 const [enabled,setEnabled]=useState(false),[url,setUrl]=useState(""),[source,setSource]=useState<{url:string;meta:RemoteTiffInfo}>(),[level,setLevel]=useState(0),[col,setCol]=useState("0"),[row,setRow]=useState("0"),[busy,setBusy]=useState(false),[message,setMessage]=useState(""),[image,setImage]=useState("");
 const current=useRef<string|null>(null),alive=useRef(true);
 useEffect(()=>{alive.current=true;void remoteTiff.status().then(v=>{if(alive.current)setEnabled(v)}).catch(()=>{});return()=>{alive.current=false;if(current.current)void remoteTiff.cancel(current.current).catch(()=>{})}},[]);
 const run=async(f:(id:string)=>Promise<void>)=>{
  if(current.current)return;
  const id=crypto.randomUUID();current.current=id;setBusy(true);setMessage("");
  try{await f(id)}catch(e){if(alive.current)setMessage(String(e))}
  finally{current.current=null;if(alive.current)setBusy(false)}
 };
 const inspect=()=>run(async(id)=>{
  if(!window.confirm("Kết nối HTTPS trực tiếp tới nguồn công khai này? Không gửi dữ liệu dự án."))return;
  setSource(undefined);setImage("");
  const input=url.trim();const meta=await remoteTiff.inspect(handle,input,id);
  if(alive.current){setSource({url:input,meta});setLevel(0);setCol("0");setRow("0");setMessage("Nguồn đã được kiểm tra.")}
 });
 const tile=()=>run(async(id)=>{
  if(!source)return;
  const x=Number(col),y=Number(row),m=source.meta.overviews[level];if(!m)throw Error("INVALID_OVERVIEW");
  if(!Number.isInteger(x)||!Number.isInteger(y)||x<0||y<0||x>=m.columns||y>=m.rows)throw Error("INVALID_TILE");
  const b=await remoteTiff.tile(handle,source.url,source.meta.identity,level,x,y,id);
  if(!alive.current)return;
  if(b.byteLength!==m.tile_width*m.tile_height)throw Error("INVALID_TILE");
  const cv=document.createElement("canvas");cv.width=m.tile_width;cv.height=m.tile_height;
  const ctx=cv.getContext("2d");if(!ctx)throw Error("CANVAS_UNAVAILABLE");
  const data=new Uint8Array(b),out=ctx.createImageData(cv.width,cv.height);
  for(let i=0;i<data.length;i++){const n=data[i]!;out.data[i*4]=n;out.data[i*4+1]=n;out.data[i*4+2]=n;out.data[i*4+3]=255}
  ctx.putImageData(out,0,0);setImage(cv.toDataURL("image/png"));setMessage("Đã đọc một block, "+data.length+" byte pixel.");
 });
 return <details><summary>Remote TIFF Range (thử nghiệm)</summary>
 <p>TIFF/BigTIFF tiled 8-bit grayscale, không nén hoặc Deflate, tối đa 12 overview; HTTPS công khai, Range và ETag mạnh. Không hỗ trợ URL chứa thông tin đăng nhập, token hoặc chuyển hướng. Chỉ xem block gốc; chưa phải lớp COG trên bản đồ.</p>
 {!enabled&&<p>Build chưa bật remote-range.</p>}
 <input aria-label="URL TIFF công khai" value={url} onChange={e=>{setUrl(e.target.value);setSource(undefined);setImage("")}} disabled={busy||!enabled} maxLength={2048} placeholder="https://.../raster.tif"/>
 <button disabled={!enabled||busy||!url.trim()} onClick={()=>void inspect()}>Kiểm tra nguồn</button>
 {source&&<><label>Overview <select value={level} disabled={busy} onChange={e=>{setLevel(Number(e.target.value));setCol("0");setRow("0");setImage("")}}>{source.meta.overviews.map(v=><option key={v.level} value={v.level}>{v.level} · {v.width} × {v.height}</option>)}</select></label><pre>{JSON.stringify(source.meta.overviews[level],null,2)}</pre><label>Block X <input value={col} onChange={e=>setCol(e.target.value)} disabled={busy}/></label><label>Block Y <input value={row} onChange={e=>setRow(e.target.value)} disabled={busy}/></label><button disabled={busy} onClick={()=>void tile()}>Đọc block</button></>}
 {busy&&<button onClick={()=>{if(current.current)void remoteTiff.cancel(current.current).catch(e=>setMessage(String(e)))}}>Hủy yêu cầu</button>}
 {image&&<img src={image} alt="Block TIFF từ nguồn HTTPS" style={{maxWidth:"100%",maxHeight:384}}/>}<p role="status">{message}</p>
 </details>;
}
