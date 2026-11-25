import {
  createSavedQuery, executeLayerQuery, getEffectivePermissions, listSavedQueries, listSpatialLayers, loadCsrfToken,
  type LayerQueryResponse, type SavedQuery, type SpatialLayer
} from "@openvietgis/client-sdk";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { apiClient } from "../../app/api";
import { currentProtectedData, protectedQueryOptions } from "../../app/protected-query";
import { FreshAccount } from "../auth/FreshAccount";
import { ProjectToolbar } from "./ProjectToolbar";
import { availableOperators, buildQueryDefinition, type FilterRow, type QueryDefinition, type QueryOperator } from "./query-filters";

const blankRow = (): FilterRow => ({field:"",op:"eq",value:"",negate:false});
const operatorLabels: Record<QueryOperator, string> = {
  eq:"=",neq:"≠",gt:">",gte:"≥",lt:"<",lte:"≤",contains:"Chứa chuỗi",is_null:"Rỗng (NULL)",is_not_null:"Không rỗng"
};
interface ExecutedQuery {
  layerId: string;
  revision: string;
  definition: QueryDefinition & {sort_field?: string; sort_direction?: string};
  sourceName: string;
  result: LayerQueryResponse;
}
function QueryWorkbench({organizationId,projectId,accountId}:{organizationId:string;projectId:string;accountId:string}) {
  const qc = useQueryClient();
  const prefix = ["r007",accountId,organizationId,projectId];
  const [layerId,setLayerId] = useState("");
  const [rows,setRows] = useState<FilterRow[]>([blankRow()]);
  const [group,setGroup] = useState<"and"|"or">("and");
  const [refLayerId,setRefLayerId] = useState("");
  const [predicate,setPredicate] = useState<"intersects"|"within"|"contains">("intersects");
  const [queryName,setQueryName] = useState("Bộ lọc đã lưu");
  const [executed,setExecuted] = useState<ExecutedQuery|null>(null);
  const [error,setError] = useState<string|null>(null);
  const [notice,setNotice] = useState<string|null>(null);
  const [working,setWorking] = useState(false);
  const lock = useRef(false);

  const layersQuery = useQuery({
    queryKey:[...prefix,"layers"], queryFn:()=>listSpatialLayers(apiClient,organizationId,projectId,{limit:100}),
    ...protectedQueryOptions
  });
  const savedQuery = useQuery({
    queryKey:[...prefix,"saved"], queryFn:()=>listSavedQueries(apiClient,organizationId,projectId),
    ...protectedQueryOptions
  });
  const permissionsQuery = useQuery({
    queryKey:[...prefix,"permissions"], queryFn:()=>getEffectivePermissions(apiClient,organizationId,{project_id:projectId}),
    ...protectedQueryOptions
  });
  const canSave = currentProtectedData(permissionsQuery)?.permissions.includes("project.edit") === true;
  const layerList = currentProtectedData(layersQuery);
  const layers = (layerList?.items ?? []).filter(layer =>
    layer.organization_id === organizationId && layer.project_id === projectId &&
    layer.source.organization_id === organizationId && layer.source.project_id === projectId &&
    layer.source.id === layer.source_id && layer.lifecycle === "active" && layer.source.lifecycle === "active" && layer.source.validation_state === "ready" &&
    layer.capabilities.some(c=>c.name === "query" && c.available && c.technical_supported && c.actor_authorized));
  const activeLayer = (layerId ? layers.find(l=>l.id===layerId) : layers[0]);
  const fields = activeLayer?.source.schema.fields ?? [];
  const activeId = activeLayer?.id ?? "";
  const currentResult = activeLayer && executed && executed.layerId === activeId &&
    executed.revision === activeLayer.source.content_revision &&
    (!executed.result.schema_revision || executed.result.schema_revision === activeLayer.source.schema_revision) ? executed : null;

  function changeRows(next:FilterRow[]) {
    setRows(next);setExecuted(null);
  }
  function patchRow(index:number,patch:Partial<FilterRow>) {
    changeRows(rows.map((row,i)=>i===index?{...row,...patch}:row));
  }
  function chooseLayer(id:string) {
    setLayerId(id);setRows([blankRow()]);setRefLayerId("");setExecuted(null);setError(null);setNotice(null);
  }
  function buildDefinition():QueryDefinition {
    return buildQueryDefinition(rows,fields,group,predicate,refLayerId || undefined);
  }

  async function execute(layer:SpatialLayer, definition:ExecutedQuery["definition"], offset=0, sourceName="Bộ lọc hiện tại", fingerprint?:string) {
    if (lock.current) return;
    if (offset>0&&!fingerprint) {setError("Tải lại truy vấn để lấy mã phiên bản phân trang.");return;}
    lock.current=true;setWorking(true);setError(null);setNotice(null);
    const revision = layer.source.content_revision;
    try {
      const csrf=await loadCsrfToken(apiClient);
      const result=await executeLayerQuery(apiClient,organizationId,projectId,layer.id,{
        ...definition, expected_content_revision:revision,limit:100,offset,
        ...(fingerprint?{expected_query_fingerprint:fingerprint,expected_schema_revision:layer.source.schema_revision}:{})
      },csrf);
      if (result.layer_id!==layer.id || result.source_id!==layer.source_id || result.content_revision!==revision || result.offset!==offset ||
          (result.schema_revision && result.schema_revision!==layer.source.schema_revision) ||
          (fingerprint && result.query_fingerprint!==fingerprint)) {
        throw new Error("Kết quả không đúng nguồn hoặc phiên bản yêu cầu.");
      }
      setExecuted({layerId:layer.id,revision,definition,sourceName,result});
    } catch(err) {
      setExecuted(null);setError(err instanceof Error?err.message:"Không thể chạy truy vấn.");
      await qc.invalidateQueries({queryKey:[...prefix,"layers"]});
    } finally {lock.current=false;setWorking(false);}
  }
  async function runQuery() {
    if (!activeLayer) return;
    try { await execute(activeLayer,buildDefinition()); }
    catch(err) {setError(err instanceof Error?err.message:"Bộ lọc không hợp lệ.");}
  }
  async function saveFilter() {
    if (!activeLayer || !canSave || lock.current) return;
    lock.current=true;setWorking(true);setError(null);setNotice(null);
    try {
      // The saved definition contract explicitly forbids limit/offset/pins.
      const definition=buildDefinition();
      const saved=await createSavedQuery(apiClient,organizationId,projectId,
        {layer_id:activeLayer.id,name:queryName.trim(),...definition},await loadCsrfToken(apiClient));
      setNotice(`Đã lưu truy vấn ${saved.name}.`);
      await qc.invalidateQueries({queryKey:[...prefix,"saved"]});
    } catch(err) {setError(err instanceof Error?err.message:"Không thể lưu truy vấn.");}
    finally {lock.current=false;setWorking(false);}
  }
  async function runSaved(saved:SavedQuery) {
    const layer=layers.find(l=>l.id===saved.layer_id);
    if(!layer) return;
    chooseLayer(layer.id);
    await execute(layer,{
      ...(saved.attribute_filter?{attribute_filter:saved.attribute_filter}:{}),
      ...(saved.spatial_filter?{spatial_filter:saved.spatial_filter}:{}),
      ...(saved.sort_field?{sort_field:saved.sort_field}:{}),
      sort_direction:saved.sort_direction
    },0,saved.name);
  }
  const disabled=working||!activeLayer;

  return <div className="ovg-suite" data-testid="query-workbench">
    <ProjectToolbar organizationId={organizationId} projectId={projectId} active="queries" title="Truy vấn Thuộc tính & Không gian"/>
    <main className="ovg-suite-body">
      {error&&<div className="ovg-error" role="alert">{error}</div>}
      {notice&&<div className="ovg-notice" role="status">{notice}</div>}
      {layersQuery.isError&&<p role="alert">Không thể truy cập lớp, truy vấn bị khóa.</p>}
      <section className="ovg-card">
        <h2>Bộ lọc Thuộc tính có kiểu & Truy vấn Không gian</h2>
        <div className="ovg-form-row">
          <label>Lớp dữ liệu mục tiêu
            <select value={activeId} disabled={working||!layerList} onChange={e=>chooseLayer(e.target.value)}>
              {layers.map(l=><option key={l.id} value={l.id}>{l.name}</option>)}
            </select>
          </label>
          {rows.length>1&&<label>Kết hợp điều kiện
            <select value={group} disabled={disabled} onChange={e=>{setGroup(e.target.value as "and"|"or");setExecuted(null);}}>
              <option value="and">Tất cả (AND)</option><option value="or">Ít nhất một (OR)</option>
            </select>
          </label>}
        </div>
        {rows.map((row,index)=>{
          const field=fields.find(f=>f.field_id===row.field);
          const noValue=row.op==="is_null"||row.op==="is_not_null";
          return <div key={index} className="ovg-form-row" data-testid={`query-filter-${index}`}>
            <label>Trường thuộc tính
              <select disabled={disabled} value={row.field} onChange={e=>patchRow(index,{field:e.target.value,op:"eq",value:""})}>
                <option value="">Không lọc thuộc tính</option>
                {fields.map(f=><option key={f.field_id} value={f.field_id}>{f.name} ({f.type})</option>)}
              </select>
            </label>
            <label>Toán tử
              <select disabled={disabled||!field} value={row.op} onChange={e=>patchRow(index,{op:e.target.value as QueryOperator})}>
                {availableOperators(field).map(op=><option key={op} value={op}>{operatorLabels[op]}</option>)}
              </select>
            </label>
            {!noValue&&<label>Giá trị
              {field?.type==="boolean"?<select value={row.value} disabled={disabled} onChange={e=>patchRow(index,{value:e.target.value})}>
                <option value="">Chọn</option><option value="true">true</option><option value="false">false</option>
              </select>:<input disabled={disabled||!field} value={row.value} onChange={e=>patchRow(index,{value:e.target.value})}
                placeholder={field?.type==="timestamp"?"2026-11-21T10:00:00+07:00":undefined}/>}
            </label>}
            <label className="ovg-inline-check"><input type="checkbox" disabled={disabled||!field} checked={row.negate}
              onChange={e=>patchRow(index,{negate:e.target.checked})}/>Phủ định (NOT)</label>
            <button type="button" disabled={disabled||rows.length===1} onClick={()=>changeRows(rows.filter((_,i)=>i!==index))}>Xóa điều kiện</button>
          </div>;
        })}
        <button type="button" className="ovg-btn ovg-btn-secondary" disabled={disabled||rows.length>=8||rows.some(r=>!r.field)}
          onClick={()=>changeRows([...rows,blankRow()])}>Thêm điều kiện</button>
        <div className="ovg-form-row">
          <label>Quan hệ không gian
            <select value={predicate} disabled={disabled} onChange={e=>{setPredicate(e.target.value as typeof predicate);setExecuted(null);}}>
              <option value="intersects">Giao nhau (Intersects)</option><option value="within">Nằm trong (Within)</option><option value="contains">Chứa (Contains)</option>
            </select>
          </label>
          <label>Lớp tham chiếu (tùy chọn)
            <select value={refLayerId} disabled={disabled} onChange={e=>{setRefLayerId(e.target.value);setExecuted(null);}}>
              <option value="">Không chọn</option>{layers.map(l=><option key={l.id} value={l.id}>{l.name}</option>)}
            </select>
          </label>
          <label>Tên truy vấn lưu<input value={queryName} disabled={disabled} onChange={e=>setQueryName(e.target.value)}/></label>
          <button type="button" className="ovg-btn" disabled={disabled} onClick={()=>void runQuery()}>Chạy truy vấn</button>
          <button type="button" className="ovg-btn ovg-btn-secondary" disabled={disabled||!canSave||!queryName.trim()} onClick={()=>void saveFilter()}>Lưu truy vấn</button>
        </div>
      </section>
      {executed&&!currentResult&&<p role="status">Kết quả không còn đúng phiên bản/lớp được phép truy cập. Chạy truy vấn lại.</p>}
      {currentResult&&activeLayer&&<section className="ovg-card" data-testid="query-result">
        <h3>Kết quả: {currentResult.result.total_count} đối tượng phù hợp (phiên bản <code>{currentResult.revision.slice(0,12)}</code>)</h3>
        <p>{currentResult.sourceName} · trang {Math.floor(currentResult.result.offset/currentResult.result.limit)+1}</p>
        <table className="ovg-table"><thead><tr><th>Mã đối tượng</th><th>Thuộc tính</th></tr></thead>
          <tbody>{currentResult.result.items.map(item=><tr key={item.feature_id}><td><code>{item.feature_id}</code></td><td><code>{JSON.stringify(item.attributes)}</code></td></tr>)}</tbody>
        </table>
        <button type="button" disabled={working||currentResult.result.offset===0}
          onClick={()=>void execute(activeLayer,currentResult.definition,Math.max(0,currentResult.result.offset-currentResult.result.limit),currentResult.sourceName,currentResult.result.query_fingerprint)}>Trang trước</button>
        <button type="button" disabled={working||!currentResult.result.has_more||!currentResult.result.query_fingerprint}
          onClick={()=>void execute(activeLayer,currentResult.definition,currentResult.result.offset+currentResult.result.limit,currentResult.sourceName,currentResult.result.query_fingerprint)}>Trang sau</button>
      </section>}
      <section className="ovg-card">
        <h3>Truy vấn đã lưu ({currentProtectedData(savedQuery)?.items.length??0})</h3>
        <ul>{(currentProtectedData(savedQuery)?.items??[]).map(q=><li key={q.id}><strong>{q.name}</strong> — lớp <code>{q.layer_id}</code>{" "}
          <button type="button" disabled={working||!layers.some(l=>l.id===q.layer_id)} onClick={()=>void runSaved(q)}>Chạy lại truy vấn</button>
        </li>)}</ul>
      </section>
    </main>
  </div>;
}

export function QueryWorkbenchPage() {
  const {organizationId,projectId}=useParams();
  if(!organizationId||!projectId) return <p role="alert">Thiếu ngữ cảnh dự án.</p>;
  return <FreshAccount key={organizationId+":"+projectId}>{account=><QueryWorkbench
    key={account.id+":"+organizationId+":"+projectId} accountId={account.id}
    organizationId={organizationId} projectId={projectId}/>}</FreshAccount>;
}
