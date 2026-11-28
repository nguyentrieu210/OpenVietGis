import {collectQuerySelection,combineQuerySelection,sameKeys,selectionLayerCurrent,QUERY_SELECTION_LIMIT,type QuerySelectionMode} from "./query-selection";
import React,{useEffect,useRef,useState} from "react";
import type {LayerQueryResponse,SpatialLayer} from "@openvietgis/client-sdk";
import {availableOperators,type FilterRow,type QueryOperator} from "./workbenches/features/platform/query-filters";
import {tableTsv} from "./table-columns";
import {TABLE_QUERY_LIMIT,TABLE_QUERY_MAX_OFFSET,tableQueryBody,verifyTableQuery} from "./table-query";
type Props={layer:SpatialLayer;execute:(layer:SpatialLayer,body:Record<string,unknown>)=>Promise<LayerQueryResponse>;openFeature:(layer:SpatialLayer,id:string,isCurrent:()=>boolean)=>Promise<void>;selection?:ReadonlySet<string>;applySelection?:(layer:SpatialLayer,before:ReadonlySet<string>,next:Set<string>)=>void};
const blank=():FilterRow=>({field:"",op:"eq",value:"",negate:false});
const labels:Record<QueryOperator,string>={eq:"=",neq:"≠",gt:">",gte:"≥",lt:"<",lte:"≤",contains:"Chứa (phân biệt hoa thường)",is_null:"NULL",is_not_null:"Không NULL"};
export function AttributeServerQuery({layer,execute,openFeature,selection,applySelection}:Props){
 const fields=layer.source.schema.fields;
 const [filters,setFilters]=useState<FilterRow[]>([blank()]),[group,setGroup]=useState<"and"|"or">("and");
 const [sort,setSort]=useState(""),[direction,setDirection]=useState<"asc"|"desc">("asc");
 const [result,setResult]=useState<LayerQueryResponse|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const [selectionMode,setSelectionMode]=useState<QuerySelectionMode>("replace"),[selectionProgress,setSelectionProgress]=useState(""),[selecting,setSelecting]=useState(false);
 const latest=useRef({layer,selection,applySelection});latest.current={layer,selection,applySelection};
 const epoch=useRef(0),lock=useRef(false);
 useEffect(()=>()=>{epoch.current++;lock.current=false},[]);
 const invalidate=()=>{epoch.current++;lock.current=false;setBusy(false);setResult(null);setError("");setSelectionProgress("");setSelecting(false)};
 const patch=(index:number,value:Partial<FilterRow>)=>{invalidate();setFilters(old=>old.map((row,i)=>i===index?{...row,...value}:row))};
 async function run(offset=0,paging=false){
  if(lock.current)return;const seq=++epoch.current;lock.current=true;setBusy(true);setError("");
  const fingerprint=paging?result?.query_fingerprint:undefined;
  setResult(null);
  try{const body=tableQueryBody(layer,filters,sort,direction,offset,fingerprint,group);const next=verifyTableQuery(layer,await execute(layer,body),offset,fingerprint);if(epoch.current===seq)setResult(next)}
  catch(e){if(epoch.current===seq)setError(e instanceof Error?e.message:"Không tải được truy vấn.")}
  finally{if(epoch.current===seq){lock.current=false;setBusy(false)}}
 }
 async function open(id:string){
  if(lock.current)return;const seq=++epoch.current;lock.current=true;setBusy(true);setError("");
  try{await openFeature(layer,id,()=>epoch.current===seq)}catch(e){if(epoch.current===seq)setError(e instanceof Error?e.message:"Không mở được đối tượng.")}
  finally{if(epoch.current===seq){lock.current=false;setBusy(false)}}
 }
 async function selectMatches(){
  if(lock.current||!result||!selection||!applySelection)return;
  const seq=++epoch.current,before=new Set(selection),base=layer,mode=selectionMode;
  lock.current=true;setBusy(true);setSelecting(true);setSelectionProgress("");setError("");
  const current=()=>epoch.current===seq&&selectionLayerCurrent(latest.current.layer,base)&&!!latest.current.selection&&sameKeys(latest.current.selection,before);
  try{
   const found=await collectQuerySelection(base,result,filters,sort,direction,group,mode,body=>execute(base,body),current,(n,total)=>{if(current())setSelectionProgress("Đã kiểm tra "+n+"/"+total+" FID…")});
   if(!current()||!latest.current.applySelection)throw Error("Phạm vi đã thay đổi.");
   const next=combineQuerySelection(before,base.source_id,found.keys,mode);
   latest.current.applySelection(base,before,next);
   setSelectionProgress("Đã cập nhật vùng chọn: "+[...next].filter(k=>k.startsWith(base.source_id+":")).length+" đối tượng nguồn này; "+found.missingGeometry+" bản ghi thiếu/không hợp lệ về hình học trong tập đã quét.");
  }catch(e){if(epoch.current===seq){setSelectionProgress("");setError(e instanceof Error?e.message:"Không áp dụng được vùng chọn.")}}
  finally{if(epoch.current===seq){lock.current=false;setBusy(false);setSelecting(false)}}
 }
 const selectionLimit=selectionMode==="invert"?layer.source.feature_count:result?.total_count;
 return <div className="attribute-server-query">
  <p className="live-note">Truy vấn thuộc tính Rust trên toàn nguồn được phép (tối đa 50.000 bản ghi), gồm cả bản ghi thiếu hoặc lỗi hình học. Không tải hình học vào kết quả; chỉ mở bản ghi có hình học hiện hữu trên bản đồ.</p>
  <form onSubmit={e=>{e.preventDefault();void run()}}>
   {filters.length>1&&<label>Kết hợp điều kiện <select value={group} disabled={busy} onChange={e=>{invalidate();setGroup(e.target.value==="or"?"or":"and")}}><option value="and">Tất cả (AND)</option><option value="or">Ít nhất một (OR)</option></select></label>}
   {filters.map((filter,index)=>{const field=fields.find(f=>f.field_id===filter.field);return <div className="attribute-filter-row" key={index}>
   <label>Trường lọc {index+1}<select value={filter.field} disabled={busy} onChange={e=>patch(index,{field:e.target.value,op:"eq",value:"",negate:false})}><option value="">{filters.length===1?"Không lọc":"Chọn trường"}</option>{fields.map(f=><option key={f.field_id} value={f.field_id}>{f.name}</option>)}</select></label>
   {field&&<><label>Toán tử <select disabled={busy} value={filter.op} onChange={e=>patch(index,{op:e.target.value as QueryOperator,value:""})}>{availableOperators(field).map(op=><option key={op} value={op}>{labels[op]}</option>)}</select></label>{filter.op!=="is_null"&&filter.op!=="is_not_null"&&<label>Giá trị <input disabled={busy} value={filter.value} onChange={e=>patch(index,{value:e.target.value})}/></label>}<label><input type="checkbox" checked={filter.negate} disabled={busy} onChange={e=>patch(index,{negate:e.target.checked})}/> Phủ định (NOT)</label></>}
   {filters.length>1&&<button type="button" disabled={busy} aria-label={"Xóa điều kiện "+(index+1)} onClick={()=>{invalidate();setFilters(old=>old.filter((_,i)=>i!==index))}}>×</button>}</div>})}
   <button type="button" disabled={busy||filters.length>=6||filters.some(f=>!f.field)} onClick={()=>{invalidate();setFilters(old=>[...old,blank()])}}>+ Điều kiện</button>
   <label>Sắp xếp <select disabled={busy} value={sort} onChange={e=>{invalidate();setSort(e.target.value)}}><option value="">FID</option>{fields.map(f=><option key={f.field_id} value={f.field_id}>{f.name}</option>)}</select></label>
   <label>Thứ tự <select disabled={busy} value={direction} onChange={e=>{invalidate();setDirection(e.target.value==="desc"?"desc":"asc")}}><option value="asc">Tăng dần</option><option value="desc">Giảm dần</option></select></label>
   <button type="submit" disabled={busy}>Chạy truy vấn</button>
  </form>
  {busy&&!selecting&&<p role="status">Đang tải từ máy chủ…</p>}{selectionProgress&&<p role="status">{selectionProgress}</p>}{error&&<p role="alert">{error}</p>}
  {result&&<><p role="status">{result.total_count.toLocaleString("vi-VN")} đối tượng phù hợp · trang {Math.floor(result.offset/TABLE_QUERY_LIMIT)+1}</p>
   <div className="qgis-table-scroll"><table><thead><tr><th>FID</th><th>Hình học</th>{fields.map(f=><th key={f.field_id}>{f.name}</th>)}</tr></thead><tbody>{result.items.map(r=><tr key={r.feature_id}><td><button type="button" disabled={busy||r.geometry_state!=="present"} onClick={()=>void open(r.feature_id)}>{r.feature_id}</button></td><td>{({present:"Có hình học",absent:"Thiếu hình học",empty:"Rỗng",invalid:"Không hợp lệ",unsupported:"Chưa hỗ trợ"} as const)[r.geometry_state??"unsupported"]}</td>{fields.map(f=><td key={f.field_id}>{String(r.attributes[f.field_id]??"—")}</td>)}</tr>)}</tbody></table></div>
   {!result.items.length&&<p>Không có đối tượng phù hợp.</p>}
   {selection&&applySelection&&<section aria-label="Vùng chọn theo truy vấn"><label>Thao tác vùng chọn<select disabled={busy} value={selectionMode} onChange={e=>{setSelectionMode(e.target.value as QuerySelectionMode);setSelectionProgress("")}}><option value="replace">Thay vùng chọn nguồn này</option><option value="add">Thêm kết quả</option><option value="remove">Bỏ kết quả khỏi vùng chọn</option><option value="intersect">Giao với vùng chọn</option><option value="invert">Đảo vùng chọn toàn nguồn (bỏ bộ lọc)</option></select></label><button type="button" disabled={busy||selectionLimit==null||selectionLimit>QUERY_SELECTION_LIMIT} onClick={()=>void selectMatches()}>Áp dụng vùng chọn trên toàn kết quả</button><small>Giới hạn 5.000 FID. Giữ vùng chọn nguồn khác; bản ghi chưa nạp hoặc thiếu hình học có thể được chọn theo FID nhưng không tự tải lên bản đồ. Công cụ sửa/xuất vẫn kiểm tra đủ dữ liệu.</small></section>}
   <footer><button type="button" disabled={busy||!result.items.length} onClick={()=>void navigator.clipboard.writeText(tableTsv(["FID","geometry_state",...fields.map(f=>f.name)],result.items.map(r=>[r.feature_id,r.geometry_state,...fields.map(f=>r.attributes[f.field_id])]))).catch(()=>setError("Không sao chép được trang."))}>Sao chép trang truy vấn</button><button type="button" disabled={busy||result.offset===0} onClick={()=>void run(result.offset-TABLE_QUERY_LIMIT,true)}>‹ Trang trước</button><button type="button" disabled={busy||!result.has_more||result.offset+TABLE_QUERY_LIMIT>TABLE_QUERY_MAX_OFFSET} onClick={()=>void run(result.offset+TABLE_QUERY_LIMIT,true)}>Trang sau ›</button></footer>
  </>}
 </div>
}
