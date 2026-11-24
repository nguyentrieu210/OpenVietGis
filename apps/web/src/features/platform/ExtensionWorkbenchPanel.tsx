import {
  activateExtension, createExtensionRecord, deactivateExtension, exportExtensionBundle, getEffectivePermissions,
  getExtensionActivationHistory, getExtensionRecordHistory, getSpatialLayer, importExtensionBundle,
  listExtensionManifests, listExtensionRecords, listProjectActivations, listSpatialLayers,
  loadCsrfToken, transitionExtension, updateExtensionRecord,
  type ExtensionActivation, type ExtensionLinkContext, type ExtensionManifest, type ExtensionPayload,
  type ExtensionPortableBundle, type ExtensionRecord, type SpatialLayer
} from "@openvietgis/client-sdk";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { apiClient } from "../../app/api";
import { currentProtectedData, protectedQueryOptions } from "../../app/protected-query";
import { FreshAccount } from "../auth/FreshAccount";

type Scope = { organizationId: string; projectId: string };
const errorText = (error: unknown) => error instanceof Error ? error.message : "Thao tác không thành công.";

interface RecordFormProps extends Scope {
  manifest: ExtensionManifest;
  record: ExtensionRecord | undefined;
  entities: string[];
  layers: SpatialLayer[];
  disabled: boolean;
  save: (entity: string, payload: ExtensionPayload, layerId: string, featureId: string, pin: ExtensionLinkContext | undefined) => Promise<void>;
}

function RecordForm({ organizationId, projectId, manifest, record, entities, layers, disabled, save }: RecordFormProps) {
  const [entity, setEntity] = useState(record?.entity_name ?? entities[0] ?? "");
  const [values, setValues] = useState<ExtensionPayload>(record?.payload ?? {});
  const [layerId, setLayerId] = useState(record?.linked_layer_id ?? "");
  const [featureId, setFeatureId] = useState(record?.linked_feature_id ?? "");
  const [pin, setPin] = useState<ExtensionLinkContext | undefined>(record?.link_context?.status === "pinned" ? record.link_context : undefined);
  const [loadingPin, setLoadingPin] = useState(false);
  const [error, setError] = useState("");
  const fields = manifest.entity_schemas[entity]?.fields ?? {};

  async function resolvePin() {
    if (!layerId) return;
    setLoadingPin(true); setError(""); setPin(undefined);
    try {
      const layer = await getSpatialLayer(apiClient, organizationId, projectId, layerId);
      const source = layer.source;
      if (layer.id !== layerId || layer.organization_id !== organizationId || layer.project_id !== projectId ||
          source.organization_id !== organizationId || source.project_id !== projectId ||
          !source.stored_crs || source.lifecycle !== "active" || source.validation_state !== "ready") {
        throw new Error("Lớp hoặc CRS không còn được xác nhận.");
      }
      const next: ExtensionLinkContext = {status: "pinned", source_id: source.id,
        content_revision: source.content_revision, schema_revision: source.schema_revision, stored_crs: source.stored_crs.code};
      if (window.confirm("Ghim rõ phiên bản nguồn hiện tại EPSG:" + next.stored_crs + " · " +
          next.content_revision.slice(0, 12) + "? Máy chủ sẽ kiểm tra lại liên kết khi lưu.")) setPin(next);
    } catch (err) {setError(errorText(err));} finally {setLoadingPin(false);}
  }

  async function submit() {
    setError("");
    try {
      const payload: ExtensionPayload = {};
      for (const [key, spec] of Object.entries(fields)) {
        const input = values[key];
        if (input == null || (input === "" && spec.type !== "text")) {
          if (spec.required) throw new Error("Thiếu trường bắt buộc: " + key);
          payload[key] = null; continue;
        }
        if (spec.type === "text") payload[key] = String(input);
        else if (spec.type === "boolean") {
          if (input !== "true" && input !== "false" && typeof input !== "boolean") throw new Error("Chọn giá trị logic: " + key);
          payload[key] = input === true || input === "true";
        } else if (spec.type === "integer" || spec.type === "number") {
          const number = Number(input);
          if (!Number.isFinite(number) || Math.abs(number) > Number.MAX_SAFE_INTEGER ||
              (spec.type === "integer" && !Number.isSafeInteger(number))) throw new Error("Số không hợp lệ: " + key);
          payload[key] = number;
        } else throw new Error("Kiểu trường không được hỗ trợ: " + key);
      }
      if (layerId && !pin) throw new Error("Phải ghim phiên bản nguồn trước khi lưu liên kết.");
      await save(entity, payload, layerId, featureId, pin);
    } catch (err) {setError(errorText(err));}
  }

  return <form className="ovg-card" onSubmit={e => {e.preventDefault(); void submit();}}>
    <h3>{record ? "Chỉnh thuộc tính mô-đun · phiên bản " + record.revision : "Tạo bản ghi mô-đun"}</h3>
    <label>Thực thể
      <select value={entity} disabled={disabled || Boolean(record)} onChange={e => {setEntity(e.target.value);setValues({});}}>
        {entities.map(name => <option key={name}>{name}</option>)}
      </select>
    </label>
    {Object.entries(fields).map(([key, spec]) => <div key={key} className="ovg-form-row">
      <label>{key} · {spec.type}{spec.required ? " (bắt buộc)" : ""}
        {spec.type === "boolean" ?
          <select value={values[key] == null ? "" : String(values[key])} disabled={disabled}
            onChange={e => setValues(v => ({...v, [key]: e.target.value || null}))}>
            <option value="">NULL</option><option value="true">Có</option><option value="false">Không</option>
          </select> :
          <input value={values[key] == null ? "" : String(values[key])} disabled={disabled || values[key] === null}
            onChange={e => setValues(v => ({...v, [key]: e.target.value}))}/>}
      </label>
      {!spec.required && spec.type !== "boolean" &&
        <label><input type="checkbox" disabled={disabled} checked={values[key] === null}
          onChange={e => setValues(v => ({...v, [key]: e.target.checked ? null : ""}))}/> NULL</label>}
    </div>)}
    <div className="ovg-form-row">
      <label>Lớp liên kết (tùy chọn)
        <select value={layerId} disabled={disabled || Boolean(record) || loadingPin}
          onChange={e => {setLayerId(e.target.value);setFeatureId("");setPin(undefined);}}>
          <option value="">Không liên kết không gian</option>
          {record?.linked_layer_id && !layers.some(l => l.id === record.linked_layer_id) &&
            <option value={record.linked_layer_id}>Liên kết đã ghi: {record.linked_layer_id}</option>}
          {layers.map(layer => <option key={layer.id} value={layer.id}>{layer.name}</option>)}
        </select>
      </label>
      {layerId && <label>Mã đối tượng Core (tùy chọn)
        <input value={featureId} disabled={disabled || Boolean(record) || loadingPin}
          onChange={e => {setFeatureId(e.target.value);setPin(undefined);}}/>
      </label>}
      {layerId && <button type="button" className="ovg-btn ovg-btn-secondary" disabled={disabled || loadingPin}
        onClick={() => void resolvePin()}>Ghim phiên bản liên kết</button>}
    </div>
    {pin && <p>Ghim nguồn {pin.source_id} · EPSG:{pin.stored_crs} · phiên bản {pin.content_revision.slice(0,12)}.</p>}
    {record && <p>Mã bản ghi: {record.id}. Không sửa hình học Core trong biểu mẫu này.</p>}
    {error && <p role="alert">{error}</p>}
    <button type="submit" className="ovg-btn" disabled={disabled || loadingPin || !entity}>
      {record ? "Lưu thay đổi theo phiên bản" : "Tạo bản ghi"}
    </button>
  </form>;
}

function ExtensionWorkspace({ organizationId, projectId, accountId }: Scope & {accountId: string}) {
  const qc = useQueryClient();
  const prefix = ["r014", accountId, organizationId, projectId];
  const [selectedKey, setSelectedKey] = useState("");
  const [selectedManifest, setSelectedManifest] = useState("");
  const [recordId, setRecordId] = useState("");
  const [importData, setImportData] = useState<ExtensionPortableBundle | null>(null);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [generation, setGeneration] = useState(0);
  const [search, setSearch] = useState("");
  const [rawToolsOpen, setRawToolsOpen] = useState(false);
  const lock = useRef(false);

  const permissions = useQuery({queryKey: [...prefix, "permissions"],
    queryFn: () => getEffectivePermissions(apiClient, organizationId, {project_id: projectId}), ...protectedQueryOptions});
  const rights = currentProtectedData(permissions)?.permissions;
  const authorized = rights?.includes("project.read") === true;
  const canEdit = authorized && rights.includes("project.edit");
  const canManage = authorized && rights.includes("project.manage");
  const manifestsQuery = useQuery({queryKey: [...prefix, "manifests"],
    queryFn: () => listExtensionManifests(apiClient, organizationId), enabled: authorized, ...protectedQueryOptions});
  const activationsQuery = useQuery({queryKey: [...prefix, "activations"],
    queryFn: () => listProjectActivations(apiClient, organizationId, projectId), enabled: authorized, ...protectedQueryOptions});
  const manifests = authorized ? currentProtectedData(manifestsQuery)?.items ?? [] : [];
  const activations = authorized ? currentProtectedData(activationsQuery)?.items ?? [] : [];
  const keys = [...new Set(manifests.map(m => m.extension_key))];
  const key = selectedKey && keys.includes(selectedKey) ? selectedKey : keys[0] ?? "";
  const activation = activations.find(a => a.extension_key === key);
  const candidates = manifests.filter(m => m.extension_key === key);
  const candidate = candidates.find(m => m.id === selectedManifest) ?? candidates.find(m => m.id === activation?.manifest_id) ?? candidates[0];
  const activeManifest = candidates.find(m => m.id === activation?.manifest_id);
  const active = Boolean(activation?.status === "active" && activeManifest?.enabled && activeManifest.api_compatibility === "1.x");
  const recordsQuery = useQuery({queryKey: [...prefix, "records", key],
    queryFn: () => listExtensionRecords(apiClient, organizationId, projectId, key),
    enabled: authorized && Boolean(activation), ...protectedQueryOptions});
  const records = authorized && activation ? currentProtectedData(recordsQuery)?.items ?? [] : [];
  const record = records.find(item => item.id === recordId);
  const historyQuery = useQuery({queryKey: [...prefix, "history", key, recordId, record?.revision],
    queryFn: () => getExtensionRecordHistory(apiClient, organizationId, projectId, key, recordId),
    enabled: authorized && Boolean(activation && record), ...protectedQueryOptions});
  const lifecycle = useQuery({queryKey: [...prefix, "lifecycle", activation?.id, activation?.updated_at],
    queryFn: () => getExtensionActivationHistory(apiClient, organizationId, projectId, activation!.id),
    enabled: authorized && Boolean(activation), ...protectedQueryOptions});
  const layersQuery = useQuery({queryKey: [...prefix, "layers"],
    queryFn: () => listSpatialLayers(apiClient, organizationId, projectId, {limit:100}),
    enabled: canEdit && active, ...protectedQueryOptions});
  const layers = currentProtectedData(layersQuery)?.items ?? [];

  async function run(action: () => Promise<string>) {
    if (lock.current || !authorized) return;
    lock.current = true; setBusy(true); setNotice(""); setError("");
    try {setNotice(await action()); await qc.invalidateQueries({queryKey: prefix});}
    catch(err) {setError(errorText(err));}
    finally {lock.current = false;setBusy(false);}
  }
  async function saveRecord(entity: string, payload: ExtensionPayload, layerId: string, featureId: string, pin: ExtensionLinkContext | undefined) {
    if (!active || !canEdit || !activeManifest || lock.current) return;
    await run(async () => {
      const csrf = await loadCsrfToken(apiClient);
      const made = record
        ? await updateExtensionRecord(apiClient, organizationId, projectId, key, record.id,
            {expected_revision:record.revision,expected_manifest_id:activeManifest.id,payload,...(pin ? {expected_link_context:pin} : {})},csrf)
        : await createExtensionRecord(apiClient, organizationId, projectId, key,
            {entity_name:entity,payload,...(layerId?{linked_layer_id:layerId}:{}),...(featureId?{linked_feature_id:featureId}:{}),...(pin?{expected_link_context:pin}:{})},csrf);
      setRecordId(made.id);setGeneration(n=>n+1);
      return "Đã lưu bản ghi phiên bản " + made.revision + ".";
    });
  }
  async function chooseImport(file: File | undefined) {
    setImportData(null);setError("");
    if (!file) return;
    try {
      if (file.size > 2.5 * 1024 * 1024) throw new Error("Gói vượt giới hạn tải.");
      const data = JSON.parse(await file.text()) as ExtensionPortableBundle;
      if (data.bundle?.format !== "openvietgis.extension.portable.v1" || data.bundle.extension_key !== key ||
          !Array.isArray(data.bundle.records) || !data.bundle.records.length || !/^[a-f0-9]{64}$/.test(data.sha256)) {
        throw new Error("Gói không đúng định dạng hoặc mô-đun.");
      }
      setImportData(data);
    } catch(err) {setError(errorText(err));}
  }
  function download() {
    void run(async () => {
      const data = await exportExtensionBundle(apiClient, organizationId, projectId, key);
      const url = URL.createObjectURL(new Blob([JSON.stringify(data)], {type:"application/json"}));
      const link = document.createElement("a");link.href = url;link.download = key + "-portable.json";
      link.click();window.setTimeout(() => URL.revokeObjectURL(url), 1000);
      return "Đã xuất " + data.bundle.records.length + " bản ghi kèm lịch sử. Mã SHA-256: " + data.sha256;
    });
  }
  function activate(target: ExtensionManifest) {
    if (!canManage) return;
    void run(async () => {
      const act = await activateExtension(apiClient,organizationId,projectId,target.id,await loadCsrfToken(apiClient));
      return "Đã kích hoạt " + act.extension_key + "@" + target.version;
    });
  }
  function disable(existing: ExtensionActivation) {
    if (!canManage || !window.confirm("Vô hiệu mô-đun; dữ liệu và lịch sử vẫn được giữ?")) return;
    void run(async() => {
      await deactivateExtension(apiClient,organizationId,projectId,existing.id,await loadCsrfToken(apiClient));
      setRecordId(""); return "Đã vô hiệu chức năng; vẫn có thể đọc và xuất dữ liệu theo quyền.";
    });
  }

  if (!authorized) return <p role="status">Chưa xác nhận quyền truy cập mô-đun của dự án.</p>;
  return <section className="ovg-card" data-testid="extension-workbench">
    <h2>Mô-đun mở rộng nội bộ</h2>
    <p>Manifest chỉ mô tả cấu trúc, quyền yêu cầu và tên lệnh. Bản ghi dùng quyền đọc/sửa dự án của Core; chưa có cấp quyền riêng theo mô-đun hoặc thực thi lệnh. Kích hoạt chỉ áp dụng cho dữ liệu dự án, không phải phê duyệt cài đặt hay chứng nhận mô-đun/SDK công khai.</p>
    {error && <p role="alert">{error}</p>}
    {notice && <p role="status">{notice}</p>}
    {!keys.length && <p>Chưa thấy manifest được đăng ký trong tổ chức.</p>}
    {keys.length > 0 && <>
      <div className="ovg-form-row">
        <label>Mô-đun
          <select value={key} disabled={busy} onChange={e=>{setSelectedKey(e.target.value);setSelectedManifest("");setRecordId("");setImportData(null);setNotice("");setError("");}}>
            {keys.map(k=><option key={k}>{k}</option>)}
          </select>
        </label>
        <label>Phiên bản manifest
          <select value={candidate?.id ?? ""} disabled={busy} onChange={e=>setSelectedManifest(e.target.value)}>
            {candidates.map(m=><option key={m.id} value={m.id}>{m.version} · {m.display_name}</option>)}
          </select>
        </label>
        {candidate && candidate.enabled && (!activation || activation.manifest_id === candidate.id && activation.status === "disabled") &&
          <button type="button" className="ovg-btn" disabled={busy || !canManage} onClick={()=>activate(candidate)}>Kích hoạt</button>}
        {activation?.status === "active" && <button type="button" className="ovg-btn ovg-btn-secondary" disabled={busy || !canManage}
          onClick={()=>disable(activation)}>Vô hiệu</button>}
        {activation && candidate && candidate.id !== activation.manifest_id &&
          <button type="button" className="ovg-btn ovg-btn-secondary" disabled={busy || !canManage}
            onClick={()=>{if(window.confirm("Chuyển tiến phiên bản cùng major, không thay schema/quyền? Máy chủ sẽ kiểm tra tương thích."))
              void run(async()=>{await transitionExtension(apiClient,organizationId,projectId,activation.id,activation.manifest_id,candidate.id,await loadCsrfToken(apiClient));
                setRecordId("");return "Đã chuyển phiên bản manifest.";});}}>Kiểm tra và chuyển phiên bản</button>}
      </div>
      <div className="mt-3 flex items-center gap-2 text-xs"><span className="font-semibold text-ink-muted">Trạng thái:</span><span className="ovg-badge">{activation ? (activation.status === "active" ? "Đang hoạt động" : "Vô hiệu") + " · v" + (activeManifest?.version ?? "?") : "Chưa kích hoạt"}</span></div>
      {candidate && <details><summary>Thông tin manifest (không phải quyền đã cấp)</summary>
        <p>{candidate.description} · API {candidate.api_compatibility}</p>
        <pre>{JSON.stringify({permissions:candidate.permissions,commands:candidate.commands,capabilities:candidate.capabilities,entity_schemas:candidate.entity_schemas},null,2)}</pre>
      </details>}
    </>}
    {activation && <details open={rawToolsOpen} onToggle={event=>setRawToolsOpen(event.currentTarget.open)}
      className="mt-4 rounded-xl border border-line bg-surface-muted p-3.5"><summary className="cursor-pointer text-xs font-bold text-ink-secondary">Công cụ kỹ thuật nâng cao (Nhập/Xuất JSON & Dữ liệu bản ghi thô)</summary><div className="mt-3 space-y-3">
      <div className="ovg-form-row">
        <button type="button" className="ovg-btn" disabled={busy} onClick={download}>Xuất dữ liệu & lịch sử JSON</button>
        <label>Chọn gói JSON để sao chép vào dự án
          <input type="file" accept="application/json,.json" disabled={busy || !canEdit || !active}
            onChange={e=>void chooseImport(e.target.files?.[0])}/>
        </label>
      </div>
      {importData && <div className="ovg-card">
        <p>{importData.bundle.records.length} bản ghi từ gói do người dùng cung cấp. Lịch sử nguồn không được nhập thành lịch sử đáng tin cậy; liên kết chéo dự án không tự ánh xạ.</p>
        <button type="button" className="ovg-btn" disabled={busy || !canEdit || !active || !activeManifest}
          onClick={()=>{if(activeManifest&&window.confirm("Tạo bản sao mới theo schema phiên bản hiện tại?")) void run(async()=>{
            const receipt=await importExtensionBundle(apiClient,organizationId,projectId,key,{...importData,expected_manifest_id:activeManifest.id},await loadCsrfToken(apiClient));
            setImportData(null);return (receipt.replayed?"Đã nhận lại biên nhận":"Đã sao chép")+" · "+receipt.mappings.length+" bản ghi.";
          });}}>Xác nhận nhập bản sao</button>
      </div>}
      <h3>Bản ghi (tối đa 200 bản ghi gần nhất; xuất từ chối nếu vượt giới hạn)</h3>
      <label>Lọc bản ghi đang hiển thị<input value={search} onChange={e=>setSearch(e.target.value)} /></label>
      <ul>{records.filter(item=>JSON.stringify(item.payload).toLocaleLowerCase().includes(search.toLocaleLowerCase())).map(item=><li key={item.id}>
        <button type="button" disabled={busy} onClick={()=>{setRecordId(item.id);setGeneration(n=>n+1);}}>
          {item.entity_name} · rev {item.revision} · {item.id.slice(0,8)}
        </button>{" "}<code>{JSON.stringify(item.payload)}</code>
      </li>)}</ul>
      {active && canEdit && <button type="button" className="ovg-btn ovg-btn-secondary" disabled={busy} onClick={()=>{setRecordId("");setGeneration(n=>n+1);}}>Tạo bản ghi mới</button>}
      {active && activeManifest && canEdit && <RecordForm key={key+":"+activation.manifest_id+":"+recordId+":"+String(record?.revision)+":"+generation}
        organizationId={organizationId} projectId={projectId} manifest={activeManifest}
        record={record} entities={Object.keys(activeManifest.entity_schemas)} layers={layers}
        disabled={busy || recordsQuery.fetchStatus !== "idle"} save={saveRecord}/>}
      {record && <details open><summary>Lịch sử bản ghi · {record.id}</summary>
        {(currentProtectedData(historyQuery)?.items ?? []).map(item=><div key={item.revision}><p>rev {item.revision} · {item.event} · {item.occurred_at} · {item.actor_id}</p><pre>{JSON.stringify(item.payload,null,2)}</pre></div>)}
      </details>}
      <details><summary>Lịch sử kích hoạt/phiên bản</summary><pre>{JSON.stringify(currentProtectedData(lifecycle)?.items ?? [],null,2)}</pre></details>
    </div></details>}
  </section>;
}

export function ExtensionWorkbenchPanel({organizationId,projectId}: Scope) {
  return <FreshAccount>{account => <ExtensionWorkspace key={account.id+":"+organizationId+":"+projectId}
    accountId={account.id} organizationId={organizationId} projectId={projectId}/>}</FreshAccount>;
}
