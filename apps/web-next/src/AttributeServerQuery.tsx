import React,{useEffect,useRef,useState} from "react";
import type {LayerQueryResponse,SpatialLayer} from "@openvietgis/client-sdk";
import {availableOperators,type FilterRow,type QueryOperator} from "./workbenches/features/platform/query-filters";
import {TABLE_QUERY_LIMIT,TABLE_QUERY_MAX_OFFSET,tableQueryBody,verifyTableQuery} from "./table-query";
type Props={layer:SpatialLayer;execute:(layer:SpatialLayer,body:Record<string,unknown>)=>Promise<LayerQueryResponse>;openFeature:(layer:SpatialLayer,id:string,isCurrent:()=>boolean)=>Promise<void>};
const labels:Record<QueryOperator,string>={eq:"=",neq:"≠",gt:">",gte:"≥",lt:"<",lte:"≤",contains:"Chứa (phân biệt hoa thường)",is_null:"NULL",is_not_null:"Không NULL"};
export function AttributeServerQuery({layer,execute,openFeature}:Props){
 const fields=layer.source.schema.fields;
 const [filter,setFilter]=useState<FilterRow>({field:"",op:"eq",value:"",negate:false});
 const [sort,setSort]=useState(""),[direction,setDirection]=useState<"asc"|"desc">("asc");
 const [result,setResult]=useState<LayerQueryResponse|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const epoch=useRef(0),lock=useRef(false);
 useEffect(()=>()=>{epoch.current++;lock.current=false},[]);
 const invalidate=()=>{epoch.current++;lock.current=false;setBusy(false);setResult(null);setError("")};
 const field=fields.find(f=>f.field_id===filter.field);
 async function run(offset=0,paging=false){
  if(lock.current)return;const seq=++epoch.current;lock.current=true;setBusy(true);setError("");
  const fingerprint=paging?result?.query_fingerprint:undefined;
  setResult(null);
  try{const body=tableQueryBody(layer,filter,sort,direction,offset,fingerprint);const next=verifyTableQuery(layer,await execute(layer,body),offset,fingerprint);if(epoch.current===seq)setResult(next)}
  catch(e){if(epoch.current===seq)setError(e instanceof Error?e.message:"Không tải được truy vấn.")}
  finally{if(epoch.current===seq){lock.current=false;setBusy(false)}}
 }
 async function open(id:string){
  if(lock.current)return;const seq=++epoch.current;lock.current=true;setBusy(true);setError("");
  try{await openFeature(layer,id,()=>epoch.current===seq)}catch(e){if(epoch.current===seq)setError(e instanceof Error?e.message:"Không mở được đối tượng.")}
  finally{if(epoch.current===seq){lock.current=false;setBusy(false)}}
 }
 return <div className="attribute-server-query">
  <p className="live-note">Truy vấn Rust trên lớp đã chọn, chỉ đối tượng có hình học hợp lệ và được API cho phép. Không gồm bản ghi thiếu hình học. API giới hạn nguồn 50.000 đối tượng và offset 25.000.</p>
  <form onSubmit={e=>{e.preventDefault();void run()}}>
   <label>Trường lọc <select value={filter.field} disabled={busy} onChange={e=>{invalidate();setFilter({field:e.target.value,op:"eq",value:"",negate:false})}}><option value="">Không lọc</option>{fields.map(f=><option key={f.field_id} value={f.field_id}>{f.name}</option>)}</select></label>
   {field&&<><label>Toán tử <select disabled={busy} value={filter.op} onChange={e=>{invalidate();setFilter({...filter,op:e.target.value as QueryOperator,value:""})}}>{availableOperators(field).map(op=><option key={op} value={op}>{labels[op]}</option>)}</select></label>{filter.op!=="is_null"&&filter.op!=="is_not_null"&&<label>Giá trị <input disabled={busy} value={filter.value} onChange={e=>{invalidate();setFilter({...filter,value:e.target.value})}}/></label>}</>}
   <label>Sắp xếp <select disabled={busy} value={sort} onChange={e=>{invalidate();setSort(e.target.value)}}><option value="">FID</option>{fields.map(f=><option key={f.field_id} value={f.field_id}>{f.name}</option>)}</select></label>
   <label>Thứ tự <select disabled={busy} value={direction} onChange={e=>{invalidate();setDirection(e.target.value==="desc"?"desc":"asc")}}><option value="asc">Tăng dần</option><option value="desc">Giảm dần</option></select></label>
   <button type="submit" disabled={busy}>Chạy truy vấn</button>
  </form>
  {busy&&<p role="status">Đang tải từ máy chủ…</p>}{error&&<p role="alert">{error}</p>}
  {result&&<><p role="status">{result.total_count.toLocaleString("vi-VN")} đối tượng phù hợp · trang {Math.floor(result.offset/TABLE_QUERY_LIMIT)+1}</p>
   <div className="qgis-table-scroll"><table><thead><tr><th>FID</th>{fields.map(f=><th key={f.field_id}>{f.name}</th>)}</tr></thead><tbody>{result.items.map(r=><tr key={r.feature_id}><td><button type="button" disabled={busy} onClick={()=>void open(r.feature_id)}>{r.feature_id}</button></td>{fields.map(f=><td key={f.field_id}>{String(r.attributes[f.field_id]??"—")}</td>)}</tr>)}</tbody></table></div>
   {!result.items.length&&<p>Không có đối tượng phù hợp.</p>}
   <footer><button type="button" disabled={busy||result.offset===0} onClick={()=>void run(result.offset-TABLE_QUERY_LIMIT,true)}>‹ Trang trước</button><button type="button" disabled={busy||!result.has_more||result.offset+TABLE_QUERY_LIMIT>TABLE_QUERY_MAX_OFFSET} onClick={()=>void run(result.offset+TABLE_QUERY_LIMIT,true)}>Trang sau ›</button></footer>
  </>}
 </div>
}
