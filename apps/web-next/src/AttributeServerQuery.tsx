import {collectQuerySelection,collectExpressionSelection,exportSelectedAttributes,collectSpatialSelection,combineQuerySelection,sameKeys,selectionLayerCurrent,QUERY_SELECTION_LIMIT,type QuerySelectionMode,type SpatialSelectionPredicate,type SpatialSelectionInput} from "./query-selection";
import {parseExpression} from "./label-expr";
import {parseSketchPoints,type XY} from "./digitizing";
import {selectionPolygon} from "./query-selection";
import React,{useEffect,useRef,useState} from "react";
import type {LayerQueryResponse,SpatialLayer} from "@openvietgis/client-sdk";
import {availableOperators,type FilterRow,type QueryOperator} from "./workbenches/features/platform/query-filters";
import {tableTsv} from "./table-columns";
import {TABLE_QUERY_LIMIT,TABLE_QUERY_MAX_OFFSET,tableQueryBody,verifyTableQuery} from "./table-query";
type Props={exportGeometry?:(layer:SpatialLayer,selected:ReadonlySet<string>,current:()=>boolean,progress:(n:number,total:number)=>void)=>Promise<{json:string;count:number;missing:number}>;drawPolygon?:(layer:SpatialLayer,accept:(points:XY[])=>void)=>void;layer:SpatialLayer;layers?:SpatialLayer[];execute:(layer:SpatialLayer,body:Record<string,unknown>)=>Promise<LayerQueryResponse>;openFeature:(layer:SpatialLayer,id:string,isCurrent:()=>boolean)=>Promise<void>;selection?:ReadonlySet<string>;applySelection?:(layer:SpatialLayer,before:ReadonlySet<string>,next:Set<string>)=>void};
const blank=():FilterRow=>({field:"",op:"eq",value:"",negate:false});
const labels:Record<QueryOperator,string>={eq:"=",neq:"≠",gt:">",gte:"≥",lt:"<",lte:"≤",contains:"Chứa (phân biệt hoa thường)",is_null:"NULL",is_not_null:"Không NULL"};
export function AttributeServerQuery({layer,layers=[],execute,openFeature,selection,applySelection,drawPolygon,exportGeometry}:Props){
 const fields=layer.source.schema.fields;
 const [filters,setFilters]=useState<FilterRow[]>([blank()]),[group,setGroup]=useState<"and"|"or">("and");
 const [sort,setSort]=useState(""),[direction,setDirection]=useState<"asc"|"desc">("asc");
 const [result,setResult]=useState<LayerQueryResponse|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const [selectionMode,setSelectionMode]=useState<QuerySelectionMode>("replace"),[selectionProgress,setSelectionProgress]=useState(""),[selecting,setSelecting]=useState(false);
 const [expression,setExpression]=useState("");
 const [spatialMode,setSpatialMode]=useState<"bounds"|"reference"|"polygon">("bounds"),[spatialBounds,setSpatialBounds]=useState(["","","",""]),[spatialRef,setSpatialRef]=useState(""),[predicate,setPredicate]=useState<SpatialSelectionPredicate>("intersects");
 const [polygonText,setPolygonText]=useState("");
 const expressionError=(()=>{if(!expression.trim())return "";try{parseExpression(expression,fields.map(f=>f.field_id));return ""}catch(e){return e instanceof Error?e.message:String(e)}})();
 const latest=useRef({layer,layers,selection,applySelection});latest.current={layer,layers,selection,applySelection};
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
 async function selectMatches(useExpression=false){
  if(lock.current||(!useExpression&&!result)||!selection||!applySelection||useExpression&&(!expression.trim()||expressionError||selectionMode==="invert"))return;
  const seq=++epoch.current,before=new Set(selection),base=layer,mode=selectionMode;
  lock.current=true;setBusy(true);setSelecting(true);setSelectionProgress("");setError("");
  const current=()=>epoch.current===seq&&selectionLayerCurrent(latest.current.layer,base)&&!!latest.current.selection&&sameKeys(latest.current.selection,before);
  try{
   const progress=(n:number,total:number)=>{if(current())setSelectionProgress("Đã kiểm tra "+n+"/"+total+" FID…")};
   const found=useExpression?await collectExpressionSelection(base,expression,body=>execute(base,body),current,progress):await collectQuerySelection(base,result!,filters,sort,direction,group,mode,body=>execute(base,body),current,progress);
   if(!current()||!latest.current.applySelection)throw Error("Phạm vi đã thay đổi.");
   const next=combineQuerySelection(before,base.source_id,found.keys,mode);
   latest.current.applySelection(base,before,next);
   setSelectionProgress("Đã cập nhật vùng chọn: "+[...next].filter(k=>k.startsWith(base.source_id+":")).length+" đối tượng nguồn này; "+found.missingGeometry+" bản ghi thiếu/không hợp lệ về hình học trong tập đã quét.");
  }catch(e){if(epoch.current===seq){setSelectionProgress("");setError(e instanceof Error?e.message:"Không áp dụng được vùng chọn.")}}
  finally{if(epoch.current===seq){lock.current=false;setBusy(false);setSelecting(false)}}
 }
 async function selectSpatial(){
  if(lock.current||!selection||!applySelection||selectionMode==="invert")return;
  const base=layer,before=new Set(selection),mode=selectionMode,reference=layers.find(l=>l.id===spatialRef);
  let input:SpatialSelectionInput;
  if(spatialMode==="reference"){if(!reference){setError("Chọn lớp tham chiếu.");return}input={reference}}
  else if(spatialMode==="polygon"){const polygon=parseSketchPoints(polygonText);try{if(!polygon)throw Error("Nhập các cặp kinh độ, vĩ độ.");selectionPolygon(polygon);input={polygon}}catch(e){setError(e instanceof Error?e.message:String(e));return}}
  else{if(spatialBounds.some(x=>!x.trim())){setError("Nhập đủ bốn tọa độ.");return}input={bounds:spatialBounds.map(Number) as [number,number,number,number]}}
  const seq=++epoch.current;lock.current=true;setBusy(true);setSelecting(true);setSelectionProgress("");setError("");
  const current=()=>epoch.current===seq&&selectionLayerCurrent(latest.current.layer,base)&&!!latest.current.selection&&sameKeys(latest.current.selection,before)&&(spatialMode!=="reference"||!!reference&&selectionLayerCurrent(latest.current.layers.find(l=>l.id===reference.id),reference));
  try{
   const found=await collectSpatialSelection(base,input,predicate,body=>execute(base,body),current,(n,total)=>{if(current())setSelectionProgress("Đã kiểm tra "+n+"/"+total+" FID…")});
   if(!current()||!latest.current.applySelection)throw Error("Phạm vi đã thay đổi.");
   latest.current.applySelection(base,before,combineQuerySelection(before,base.source_id,found.keys,mode));
   setSelectionProgress("Đã áp dụng "+found.keys.size+" kết quả không gian.");
  }catch(e){if(epoch.current===seq){setSelectionProgress("");setError(e instanceof Error?e.message:String(e))}}
  finally{if(epoch.current===seq){lock.current=false;setBusy(false);setSelecting(false)}}
 }
 async function exportSelection(format:"tsv"|"geojson"="tsv"){
  if(lock.current||!selection)return;
  const seq=++epoch.current,before=new Set(selection),base=layer;
  lock.current=true;setBusy(true);setSelecting(true);setError("");setSelectionProgress("");
  const current=()=>epoch.current===seq&&selectionLayerCurrent(latest.current.layer,base)&&!!latest.current.selection&&sameKeys(latest.current.selection,before);
  try{
   const progress=(n:number,total:number)=>{if(current())setSelectionProgress("Đang đối chiếu "+n+"/"+total+" bản ghi…")};
   let blob:Blob,name:string,message:string;
   if(format==="geojson"){
    if(!exportGeometry)throw Error("Xuất hình học chưa khả dụng.");
    const found=await exportGeometry(base,before,current,progress);blob=new Blob([found.json],{type:"application/geo+json"});name="selected-features.geojson";message="Đã xuất "+found.count+" đối tượng · "+found.missing+" hình học NULL.";
   }else{
    const found=await exportSelectedAttributes(base,before,body=>execute(base,body),current,progress);blob=new Blob(["\ufeff",found.tsv],{type:"text/tab-separated-values;charset=utf-8"});name="selected-attributes.tsv";message="Đã xuất "+found.count+" dòng thuộc tính được chọn.";
   }
   if(!current())throw Error("Phạm vi đã thay đổi.");
   const url=URL.createObjectURL(blob);const a=document.createElement("a");a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);setSelectionProgress(message);
  }catch(e){if(epoch.current===seq){setSelectionProgress("");setError(e instanceof Error?e.message:String(e))}}
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
  {selection&&applySelection&&<details><summary>Chọn theo biểu thức AST (toàn nguồn)</summary><p>Biểu thức Boolean; NULL không được chọn. Tối đa 5.000 bản ghi, đọc theo revision đã xác minh, không thực thi SQL.</p><label>Biểu thức<textarea rows={3} maxLength={500} disabled={busy} value={expression} onChange={e=>{setExpression(e.target.value);setSelectionProgress("")}} placeholder={'"ma" IS NOT NULL'}/></label><div>{fields.slice(0,50).map(f=><button key={f.field_id} type="button" disabled={busy} onClick={()=>setExpression(v=>(v+'"'+f.field_id.replaceAll('"','""')+'"').slice(0,500))}>{f.name}</button>)}</div>{expressionError&&<p role="alert">{expressionError}</p>}<label>Thao tác<select value={selectionMode} disabled={busy} onChange={e=>setSelectionMode(e.target.value as QuerySelectionMode)}><option value="replace">Thay vùng chọn nguồn này</option><option value="add">Thêm</option><option value="remove">Bỏ</option><option value="intersect">Giao</option>{selectionMode==="invert"&&<option value="invert" disabled>Đảo (chỉ truy vấn)</option>}</select></label><button type="button" disabled={busy||!expression.trim()||!!expressionError||selectionMode==="invert"||layer.source.feature_count==null||layer.source.feature_count>QUERY_SELECTION_LIMIT} onClick={()=>void selectMatches(true)}>Áp dụng biểu thức</button></details>}
  {selection&&applySelection&&<details><summary>Chọn theo vị trí / vùng tọa độ</summary><p>PostGIS thực hiện quan hệ không gian trên nguồn và CRS đã xác minh, tối đa 5.000 kết quả. Không chọn đối tượng thiếu hình học.</p><label>Quan hệ<select disabled={busy} value={predicate} onChange={e=>setPredicate(e.target.value as SpatialSelectionPredicate)}><option value="intersects">Giao nhau</option><option value="within">Nằm trong</option><option value="contains">Chứa</option></select></label><label>Tham chiếu<select disabled={busy} value={spatialMode} onChange={e=>setSpatialMode(e.target.value==="reference"?"reference":e.target.value==="polygon"?"polygon":"bounds")}><option value="bounds">Khung WGS84</option><option value="polygon">Đa giác WGS84</option><option value="reference">Lớp khác</option></select></label>{spatialMode==="bounds"?<div>{["Tây (kinh độ)","Nam (vĩ độ)","Đông (kinh độ)","Bắc (vĩ độ)"].map((name,i)=><label key={name}>{name}<input disabled={busy} inputMode="decimal" value={spatialBounds[i]} onChange={e=>setSpatialBounds(old=>old.map((x,j)=>i===j?e.target.value:x))}/></label>)}</div>:spatialMode==="polygon"?<><label>Đỉnh WGS84 (kinh độ, vĩ độ)<textarea rows={4} maxLength={16000} disabled={busy} value={polygonText} onChange={e=>setPolygonText(e.target.value)}/></label>{drawPolygon&&<button type="button" disabled={busy} onClick={()=>{const ticket=epoch.current;drawPolygon(layer,points=>{if(epoch.current===ticket&&selectionLayerCurrent(latest.current.layer,layer))setPolygonText(points.map(p=>p.join(", ")).join("\n"))})}}>Vẽ đa giác trên bản đồ</button>}<small>3–256 đỉnh, tự khép kín khi truy vấn; không tự giao.</small></>:<label>Lớp tham chiếu<select disabled={busy} value={spatialRef} onChange={e=>setSpatialRef(e.target.value)}><option value="">Chọn lớp</option>{layers.filter(l=>l.id!==layer.id&&selectionLayerCurrent(l,l)).map(l=><option key={l.id} value={l.id}>{l.name}</option>)}</select></label>}<label>Thao tác<select disabled={busy} value={selectionMode} onChange={e=>setSelectionMode(e.target.value as QuerySelectionMode)}><option value="replace">Thay</option><option value="add">Thêm</option><option value="remove">Bỏ</option><option value="intersect">Giao</option>{selectionMode==="invert"&&<option value="invert" disabled>Đảo (chỉ truy vấn)</option>}</select></label><button type="button" disabled={busy||selectionMode==="invert"} onClick={()=>void selectSpatial()}>Áp dụng vùng chọn không gian</button></details>}
  {selection&&<section aria-label="Xuất thuộc tính vùng chọn"><button type="button" disabled={busy||![...selection].some(k=>k.startsWith(layer.source_id+":"))} onClick={()=>void exportSelection()}>Xuất toàn bộ thuộc tính đã chọn (TSV)</button>{exportGeometry&&<button type="button" disabled={busy||![...selection].some(k=>k.startsWith(layer.source_id+":"))} onClick={()=>void exportSelection("geojson")}>Xuất vùng chọn GeoJSON (WGS84)</button>}<small>Gồm cả FID chưa tải; GeoJSON giữ FID, thuộc tính và trạng thái hình học (NULL nếu không khả dụng), kiểm tra revision trước khi tải tệp.</small></section>}
  {busy&&!selecting&&<p role="status">Đang tải từ máy chủ…</p>}{selectionProgress&&<p role="status">{selectionProgress}</p>}{error&&<p role="alert">{error}</p>}
  {result&&<><p role="status">{result.total_count.toLocaleString("vi-VN")} đối tượng phù hợp · trang {Math.floor(result.offset/TABLE_QUERY_LIMIT)+1}</p>
   <div className="qgis-table-scroll"><table><thead><tr><th>FID</th><th>Hình học</th>{fields.map(f=><th key={f.field_id}>{f.name}</th>)}</tr></thead><tbody>{result.items.map(r=><tr key={r.feature_id}><td><button type="button" disabled={busy||r.geometry_state!=="present"} onClick={()=>void open(r.feature_id)}>{r.feature_id}</button></td><td>{({present:"Có hình học",absent:"Thiếu hình học",empty:"Rỗng",invalid:"Không hợp lệ",unsupported:"Chưa hỗ trợ"} as const)[r.geometry_state??"unsupported"]}</td>{fields.map(f=><td key={f.field_id}>{String(r.attributes[f.field_id]??"—")}</td>)}</tr>)}</tbody></table></div>
   {!result.items.length&&<p>Không có đối tượng phù hợp.</p>}
   {selection&&applySelection&&<section aria-label="Vùng chọn theo truy vấn"><label>Thao tác vùng chọn<select disabled={busy} value={selectionMode} onChange={e=>{setSelectionMode(e.target.value as QuerySelectionMode);setSelectionProgress("")}}><option value="replace">Thay vùng chọn nguồn này</option><option value="add">Thêm kết quả</option><option value="remove">Bỏ kết quả khỏi vùng chọn</option><option value="intersect">Giao với vùng chọn</option><option value="invert">Đảo vùng chọn toàn nguồn (bỏ bộ lọc)</option></select></label><button type="button" disabled={busy||selectionLimit==null||selectionLimit>QUERY_SELECTION_LIMIT} onClick={()=>void selectMatches()}>Áp dụng vùng chọn trên toàn kết quả</button><small>Giới hạn 5.000 FID. Giữ vùng chọn nguồn khác; bản ghi chưa nạp hoặc thiếu hình học có thể được chọn theo FID nhưng không tự tải lên bản đồ. Công cụ sửa/xuất vẫn kiểm tra đủ dữ liệu.</small></section>}
   <footer><button type="button" disabled={busy||!result.items.length} onClick={()=>void navigator.clipboard.writeText(tableTsv(["FID","geometry_state",...fields.map(f=>f.name)],result.items.map(r=>[r.feature_id,r.geometry_state,...fields.map(f=>r.attributes[f.field_id])]))).catch(()=>setError("Không sao chép được trang."))}>Sao chép trang truy vấn</button><button type="button" disabled={busy||result.offset===0} onClick={()=>void run(result.offset-TABLE_QUERY_LIMIT,true)}>‹ Trang trước</button><button type="button" disabled={busy||!result.has_more||result.offset+TABLE_QUERY_LIMIT>TABLE_QUERY_MAX_OFFSET} onClick={()=>void run(result.offset+TABLE_QUERY_LIMIT,true)}>Trang sau ›</button></footer>
  </>}
 </div>
}
