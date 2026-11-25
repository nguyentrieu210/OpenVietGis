import {
  ApiError, commitChangeset, getEffectivePermissions, getProject, listChangesets, listSpatialLayers,
  loadCsrfToken, readSpatialFeatures, revertChangeset, type Changeset, type SpatialFeature
} from "@openvietgis/client-sdk";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { apiClient } from "../../app/api";
import { currentProtectedData, protectedQueryOptions } from "../../app/protected-query";
import { FreshAccount } from "../auth/FreshAccount";
import { parseSketch, type SketchFamily } from "./editing-geometry";
import { NativeGeometryEditor } from "./NativeGeometryEditor";
import { ProjectToolbar } from "./ProjectToolbar";

function sourceKindLabel(kind: string): string {
  const labels: Record<string, string> = {
    imported_file: "Tệp đã nhập",
    external_connection: "Nguồn kết nối ngoài",
    reference_fixture: "Nguồn tham chiếu"
  };
  return labels[kind] ?? kind;
}

function editOperationLabel(kind: string): string {
  const labels: Record<string, string> = {
    create: "tạo mới",
    update: "cập nhật",
    delete: "xóa"
  };
  return labels[kind] ?? kind;
}

interface DraftOp {
  op_type: "create" | "update" | "delete";
  feature_id: string;
  geometry?: Record<string, unknown> | null;
  properties?: Record<string, unknown> | null;
}
interface DraftPin {
  sourceId: string;
  content: string;
  schema: string;
}
interface Submission {
  pin: DraftPin;
  operationId: string;
  summary: string;
  operations: DraftOp[];
}
function readObject(raw: string): Record<string, unknown> | null {
  if (!raw.trim()) return null;
  const value: unknown = JSON.parse(raw);
  if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error("Cần nhập một đối tượng JSON.");
  return value as Record<string, unknown>;
}
function familySupported(family: string): family is SketchFamily {
  return family === "Point" || family === "LineString" || family === "Polygon";
}

function EditingWorkbench({ organizationId, projectId, accountId }: { organizationId: string; projectId: string; accountId: string }) {
  const qc = useQueryClient();
  const prefix = ["r006", accountId, organizationId, projectId];
  const [selectedSourceId, setSelectedSourceId] = useState("");
  const [featureId, setFeatureId] = useState("feat_new_1");
  const [opType, setOpType] = useState<"create" | "update" | "delete">("create");
  const [geomJson, setGeomJson] = useState("");
  const [propsJson, setPropsJson] = useState("{}");
  const [summary, setSummary] = useState("Chỉnh sửa đối tượng tương tác");
  const [draft, setDraft] = useState<DraftOp[]>([]);
  const [redoStack, setRedoStack] = useState<DraftOp[]>([]);
  const [pin, setPin] = useState<DraftPin | null>(null);
  const [submission, setSubmission] = useState<Submission | null>(null);
  const [working, setWorking] = useState(false);
  const inFlight = useRef(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editorGeneration, setEditorGeneration] = useState(0);

  const projectQuery = useQuery({
    queryKey: [...prefix, "project"], queryFn: () => getProject(apiClient, organizationId, projectId),
    ...protectedQueryOptions
  });
  const permissionQuery = useQuery({
    queryKey: [...prefix, "permissions"], queryFn: () => getEffectivePermissions(apiClient, organizationId, { project_id: projectId }),
    ...protectedQueryOptions
  });
  const project = currentProtectedData(projectQuery), permissions = currentProtectedData(permissionQuery);
  const authorized = project?.id === projectId && project.organization_id === organizationId &&
    project.status === "active" && permissions?.permissions.includes("project.edit") === true;
  const layersQuery = useQuery({
    queryKey: [...prefix, "layers"],
    queryFn: () => listSpatialLayers(apiClient, organizationId, projectId, { limit: 100 }),
    enabled: authorized,
    ...protectedQueryOptions
  });
  const layerData = currentProtectedData(layersQuery);
  // Data source editability depends on authoritative layer capabilities and
  // project permissions, not merely the existence of a database object.
  const editableLayers = authorized ? (layerData?.items ?? []).filter(l =>
    l.organization_id === organizationId && l.project_id === projectId &&
    l.source.organization_id === organizationId && l.source.project_id === projectId &&
    l.source.id === l.source_id && l.lifecycle === "active" && l.source.lifecycle === "active" &&
    l.source.kind !== "reference_fixture" && l.source.validation_state === "ready" &&
    l.source.schema.dimensions === "XY" && familySupported(l.source.schema.geometry_family) &&
    l.capabilities.some(c => c.name === "edit" && c.available && c.technical_supported && c.actor_authorized)
  ).filter((l, index, all) => all.findIndex(other => other.source_id === l.source_id) === index) : [];
  const activeLayer = selectedSourceId
    ? editableLayers.find(l => l.source_id === selectedSourceId)
    : editableLayers[0];
  const activeSource = activeLayer?.source;
  const stale = Boolean(pin && activeSource && (activeSource.id !== pin.sourceId ||
    activeSource.content_revision !== pin.content || activeSource.schema_revision !== pin.schema));
  const locked = working || submission !== null;

  const historyQuery = useQuery({
    queryKey: [...prefix, "changesets", activeSource?.id, activeSource?.content_revision],
    queryFn: () => listChangesets(apiClient, organizationId, projectId, activeSource!.id),
    enabled: Boolean(activeSource?.id),
    ...protectedQueryOptions
  });
  const featuresQuery = useQuery({
    queryKey: [...prefix, "features", activeLayer?.id, activeSource?.content_revision, activeSource?.schema_revision],
    queryFn: () => readSpatialFeatures(apiClient, organizationId, projectId, activeLayer!.id, {
      content_revision: activeSource!.content_revision, schema_revision: activeSource!.schema_revision,
      include_geometry: true, limit: 50
    }),
    enabled: Boolean(activeLayer && activeSource?.stored_crs),
    ...protectedQueryOptions
  });
  const rawFeatures = currentProtectedData(featuresQuery);
  const featurePage = activeLayer && activeSource && rawFeatures &&
    rawFeatures.context.organization_id === organizationId && rawFeatures.context.project_id === projectId &&
    rawFeatures.context.layer_id === activeLayer.id && rawFeatures.context.source_id === activeSource.id &&
    rawFeatures.context.content_revision === activeSource.content_revision &&
    rawFeatures.context.schema_revision === activeSource.schema_revision &&
    rawFeatures.context.geometry_included && rawFeatures.context.response_crs?.code === activeSource.stored_crs?.code
    ? rawFeatures : undefined;
  const canModify = authorized && Boolean(activeSource && featurePage) && !locked && !stale;

  // Capture the revision when the user begins a draft based on the read
  // geometry/attributes, not after a possible refresh immediately before save.
  function bindDraftPin(): void {
    if (!pin && activeSource) {
      setPin({ sourceId: activeSource.id, content: activeSource.content_revision, schema: activeSource.schema_revision });
    }
  }

  function clearDraft(): void {
    setDraft([]);setRedoStack([]);setPin(null);setSubmission(null);setNotice(null);setError(null);
    setGeomJson("");setPropsJson("{}");setEditorGeneration(prev => prev + 1);
  }
  function stageOperation(): void {
    if (!canModify || !activeSource) return;
    setError(null);
    try {
      const fid = featureId.trim();
      if (!/^[A-Za-z0-9_-]{1,128}$/.test(fid)) throw new Error("Mã đối tượng phải có 1–128 ký tự chữ, số, gạch ngang hoặc gạch dưới.");
      if (draft.some(op => op.feature_id === fid)) throw new Error("Mỗi đối tượng chỉ có một thao tác trong đợt lưu.");
      const op: DraftOp = { op_type: opType, feature_id: fid };
      if (opType !== "delete") {
        const geometry = readObject(geomJson);
        const properties = readObject(propsJson);
        if (geometry) {
          const parsed = parseSketch(geometry);
          if (!parsed || parsed.type !== activeSource.schema.geometry_family) {
            throw new Error("Hình học XY phải khớp loại hình học và có cấu trúc hợp lệ.");
          }
          op.geometry = parsed;
        } else if (opType === "create") {
          throw new Error("Chọn hoặc vẽ hình học mới trước khi tạo đối tượng.");
        }
        if (properties) op.properties = properties;
        if (opType === "update" && !geometry && !properties) throw new Error("Chưa nhập thay đổi.");
      }
      bindDraftPin();
      setDraft(prev => [...prev, op]);
      setRedoStack([]);
      setNotice(`Đã đưa thao tác ${editOperationLabel(op.op_type)} của ${op.feature_id} vào bản nháp cục bộ.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Dữ liệu nháp không hợp lệ.");
    }
  }

  function undoDraft(): void {
    if (locked || draft.length === 0) return;
    const last = draft[draft.length - 1]!;
    setDraft(prev => prev.slice(0, -1));
    setRedoStack(prev => [last, ...prev]);
  }
  function redoDraft(): void {
    if (locked || stale || redoStack.length === 0) return;
    const [first, ...rest] = redoStack;
    if (!first) return;
    setDraft(prev => [...prev, first]);
    setRedoStack(rest);
  }

  async function saveChangeset(): Promise<void> {
    if (inFlight.current || !authorized || !activeSource || !pin || !draft.length ||
      activeSource.id !== pin.sourceId || (!submission && stale)) return;
    const request = submission ?? {
      pin, operationId: `ui-cs-${crypto.randomUUID()}`, summary, operations: draft
    };
    setSubmission(request);
    inFlight.current = true;
    setWorking(true);setError(null);setNotice(null);
    try {
      const csrf = await loadCsrfToken(apiClient);
      const cs = await commitChangeset(apiClient, organizationId, projectId, request.pin.sourceId,
        { base_content_revision: request.pin.content, base_schema_revision: request.pin.schema, operation_id: request.operationId,
          summary: request.summary, operations: request.operations }, csrf);
      if (cs.source_id !== request.pin.sourceId || cs.operation_id !== request.operationId ||
          cs.base_content_revision !== request.pin.content) {
        throw new Error("Biên nhận không khớp đợt lưu; chưa xác nhận thành công.");
      }
      setDraft([]);setRedoStack([]);setPin(null);setSubmission(null);
      setGeomJson("");setPropsJson("{}");setFeatureId(opType === "create" ? "feat_new_1" : "");setEditorGeneration(prev => prev + 1);
      setNotice(`Đã lưu đợt thay đổi ${cs.id} (phiên bản ${cs.resulting_content_revision.slice(0, 12)}).`);
      await qc.invalidateQueries({ queryKey: prefix });
    } catch (err) {
      // Definitive validation/authorization/conflict errors unlock the draft.
      // Network/5xx errors may have committed; retain the exact operation ID and
      // intent for idempotent retry rather than allowing a duplicate mutation.
      if (err instanceof ApiError && err.status >= 400 && err.status < 500) {
        setSubmission(null);
        setError("Máy chủ từ chối đợt lưu. Bản nháp được giữ nguyên; hãy kiểm tra quyền, dữ liệu hoặc phiên bản.");
        await qc.invalidateQueries({ queryKey: prefix });
      } else {
        setError("Chưa xác nhận máy chủ đã lưu hay chưa. Hãy thử lại cùng đợt lưu; không tạo thao tác trùng.");
      }
    } finally {
      inFlight.current = false;
      setWorking(false);
    }
  }

  async function handleRevert(cs: Changeset): Promise<void> {
    if (!authorized || locked || pin || inFlight.current) return;
    setError(null);inFlight.current = true;setWorking(true);
    try {
      const csrf = await loadCsrfToken(apiClient);
      const rev = await revertChangeset(apiClient, organizationId, projectId, cs.id, `ui-rev-${crypto.randomUUID()}`, csrf);
      setNotice(`Đã hoàn nguyên bộ thay đổi ${cs.id} thông qua bộ thay đổi mới ${rev.id}.`);
      await qc.invalidateQueries({ queryKey: prefix });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Hoàn nguyên không thành công.");
    } finally {
      inFlight.current = false;setWorking(false);
    }
  }

  function selectFeature(feature: SpatialFeature): void {
    if (!canModify) return;
    bindDraftPin();
    setFeatureId(feature.feature_id);
    const geometry = feature.geometry && parseSketch(feature.geometry);
    setGeomJson(geometry ? JSON.stringify(geometry) : "");
    setPropsJson(JSON.stringify(feature.attributes));
  }

  return (
    <div className="ovg-suite" data-testid="editing-workbench">
      <ProjectToolbar organizationId={organizationId} projectId={projectId} active="editing" title="Biên tập & Lịch sử thay đổi" />
      <main className="ovg-suite-body">
        {notice && <div className="ovg-notice" role="status">{notice}</div>}
        {error && <div className="ovg-error" role="alert">{error}</div>}
        {!authorized && <p role="status">Đang kiểm tra quyền biên tập hoặc dự án chỉ cho phép đọc. Không thể sửa dữ liệu.</p>}
        {authorized && layersQuery.isError && <p role="alert">Không thể truy cập danh mục lớp hiện tại.</p>}
        {authorized && layerData && !editableLayers.length && <p role="status">Không có lớp XY đang hoạt động được phép biên tập.</p>}
        {authorized && layerData?.next_after_id && <p role="status">Chỉ hiển thị trang danh mục đầu tiên (tối đa 100 lớp).</p>}
        {authorized && pin && !activeSource && !locked && <p role="alert">Nguồn của bản nháp không còn khả dụng. <button type="button" onClick={clearDraft}>Hủy bản nháp</button></p>}
        {authorized && activeLayer && activeSource && <section className="ovg-card">
          <h2>Phiên bản nháp cục bộ & Lưu nguyên tử</h2>
          <div className="ovg-form-row">
            <label>Nguồn dữ liệu có thể biên tập
              <select value={activeSource.id} disabled={locked || Boolean(pin)} onChange={e => {
                clearDraft();setSelectedSourceId(e.target.value);
              }}>
                {editableLayers.map(l => <option key={l.source_id} value={l.source_id}>
                  {l.source.name} ({sourceKindLabel(l.source.kind)}, EPSG:{l.source.stored_crs?.code ?? "Không có"})
                </option>)}
              </select>
            </label>
            <label>Thao tác
              <select value={opType} disabled={!canModify} onChange={e => {
                setOpType(e.target.value as DraftOp["op_type"]);setGeomJson("");setPropsJson("{}");
                setFeatureId(e.target.value === "create" ? "feat_new_1" : "");
              }}>
                <option value="create">Tạo đối tượng mới</option>
                <option value="update">Cập nhật đối tượng</option>
                <option value="delete">Xóa đối tượng</option>
              </select>
            </label>
            <label>Mã đối tượng
              <input value={featureId} disabled={!canModify} onChange={e => {
                if (opType !== "create") {setGeomJson("");setPropsJson("");setEditorGeneration(prev => prev + 1);}
                setFeatureId(e.target.value);
              }} />
            </label>
          </div>
          {stale && <p role="alert">Phiên bản nguồn đã thay đổi. Bản nháp bị khóa, không tự động ghi đè lên dữ liệu mới. Hủy bản nháp và đọc lại nguồn để thực hiện lại.</p>}
          {featurePage && activeSource.stored_crs && familySupported(activeSource.schema.geometry_family) && !stale &&
            <NativeGeometryEditor key={`${activeSource.id}:${activeSource.content_revision}:${opType}:${editorGeneration}`}
              source={activeSource} family={activeSource.schema.geometry_family} features={featurePage.items}
              partial={featurePage.next_after_feature_id !== null} opType={opType} featureId={featureId}
              onSelectFeature={selectFeature} onGeometry={value => { bindDraftPin();setGeomJson(JSON.stringify(value)); }} disabled={!canModify} />}
          {!featurePage && <p role="status">Đang kiểm tra hình học đúng phiên bản. {featuresQuery.isError ? "Không thể tải bản đồ biên tập hiện tại." : ""}</p>}
          {opType !== "delete" && <div className="ovg-form-row">
            <label style={{ flex: 1 }}>Hình học 2D GeoJSON
              <textarea value={geomJson} disabled={!canModify} rows={3} onChange={e => { bindDraftPin();setGeomJson(e.target.value); }}
                placeholder="Vẽ trên lưới hoặc nhập hình học X/Y CRS gốc" />
            </label>
            <label style={{ flex: 1 }}>Thuộc tính JSON có kiểu
              <textarea value={propsJson} disabled={!canModify} rows={3} onChange={e => { bindDraftPin();setPropsJson(e.target.value); }} />
            </label>
          </div>}
          {pin && <p data-testid="draft-revision">Nháp tại phiên bản {pin.content.slice(0, 12)} · {draft.length} thao tác</p>}
          {draft.length > 0 && <ul>{draft.map(op => <li key={op.feature_id}>{editOperationLabel(op.op_type)}: {op.feature_id}</li>)}</ul>}
          <div className="ovg-form-row">
            <label style={{ flex: 1 }}>Mô tả đợt chỉnh sửa
              <input value={summary} disabled={!canModify} onChange={e => setSummary(e.target.value)} />
            </label>
            <button type="button" className="ovg-btn ovg-btn-secondary" onClick={stageOperation} disabled={!canModify}>Đưa vào bản nháp</button>
            <button type="button" className="ovg-btn ovg-btn-secondary" onClick={undoDraft} disabled={locked || !draft.length}>Hoàn tác ({draft.length})</button>
            <button type="button" className="ovg-btn ovg-btn-secondary" onClick={redoDraft} disabled={locked || stale || !redoStack.length}>Làm lại ({redoStack.length})</button>
            {pin && <button type="button" className="ovg-btn ovg-btn-secondary" onClick={clearDraft} disabled={locked}>Hủy bản nháp</button>}
            <button type="button" className="ovg-btn" onClick={() => void saveChangeset()}
              disabled={!activeSource || !draft.length || working || (stale && !submission)}>
              {submission ? "Thử lại đợt lưu" : "Lưu thay đổi"}
            </button>
          </div>
        </section>}
        {authorized && activeSource && <section className="ovg-card">
          <h2>Lịch sử thay đổi bất biến</h2>
          <table className="ovg-table">
            <thead><tr><th>Mô tả</th><th>Người thực hiện</th><th>Số thao tác</th><th>Phiên bản mới</th><th>Thao tác</th></tr></thead>
            <tbody>{(currentProtectedData(historyQuery)?.items ?? []).map(cs => <tr key={cs.id}>
              <td>{cs.summary}</td><td>{cs.actor_display_snapshot}</td><td>{cs.operation_count}</td>
              <td><code>{cs.resulting_content_revision.slice(0, 12)}</code></td>
              <td><button type="button" className="ovg-btn ovg-btn-secondary" disabled={locked || Boolean(pin)}
                onClick={() => void handleRevert(cs)}>Hoàn nguyên</button></td>
            </tr>)}</tbody>
          </table>
        </section>}
      </main>
    </div>
  );
}

export function EditingWorkbenchPage() {
  const { organizationId, projectId } = useParams();
  if (!organizationId || !projectId) return <p role="alert">Thiếu ngữ cảnh dự án.</p>;
  return <FreshAccount key={organizationId + ":" + projectId}>{account =>
    <EditingWorkbench key={account.id + ":" + organizationId + ":" + projectId}
      accountId={account.id} organizationId={organizationId} projectId={projectId} />
  }</FreshAccount>;
}
