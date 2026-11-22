import {
  ApiError,
  createSpatialLayer,
  getSpatialLayer,
  listSpatialLayers,
  renameSpatialLayer,
  retireSpatialLayer,
  getProject,
  getSpatialSource,
  listSpatialSources,
  loadCsrfToken,
  registerSpatialSource,
  retireSpatialSource,
  SpatialApiError,
  updateSpatialSource,
  type SpatialSource,
  type SpatialLayer,
  type SpatialLayerMutation,
  type SpatialDeclaredCrs,
  type SpatialSourceMutation
} from "@openvietgis/client-sdk";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { Link, useParams } from "react-router-dom";

import { apiClient } from "../../app/api";
import { currentProtectedData, protectedQueryOptions } from "../../app/protected-query";
import { FreshAccount } from "../auth/FreshAccount";
import { CrsRegistryPanel } from "./CrsRegistryPanel";
import { FeatureInspector } from "./FeatureInspector";
import { CatalogHistoryPanel } from "./CatalogHistoryPanel";
import { canReadHistory, historyPrefix } from "./catalog-history";

function stateLabel(value: string): string {
  const labels: Record<string, string> = {
    active: "Đang hoạt động", archived: "Đã lưu trữ", deleted: "Đã xóa",
    ready: "Sẵn sàng", pending: "Đang chờ", invalid: "Không hợp lệ",
    known: "Đã xác định", unavailable: "Không khả dụng", empty: "Rỗng"
  };
  return labels[value] ?? value;
}

const fixtureSuggestions = [
  "reference_points_4326", "reference_lines_4326", "reference_polygons_4326",
  "reference_points_3857", "reference_points_4756", "reference_points_5899",
  "reference_empty_4326", "reference_correctable_crs"
] as const;

function safeError(error: unknown): string {
  if (error instanceof SpatialApiError) {
    switch (error.code) {
      case "STALE_LAYER_REVISION": return "Lớp dữ liệu đã thay đổi. Hãy kiểm tra phiên bản mới nhất trước khi thử lại.";
      case "SOURCE_NOT_READY": return "Nguồn dữ liệu chưa sẵn sàng để tạo lớp.";
      case "STALE_CATALOG_REVISION": return "Nguồn dữ liệu đã thay đổi. Hãy kiểm tra phiên bản mới nhất trước khi thử lại.";
      case "ACTIVE_LAYER_DEPENDENCIES": return "Cần ngừng sử dụng các lớp đang hoạt động trước khi ngừng nguồn dữ liệu.";
      case "SOURCE_ALREADY_REGISTERED": return "Tham chiếu này đã được đăng ký trong dự án, kể cả nguồn đã ngừng sử dụng.";
      case "PROJECT_NOT_WRITABLE": return "Dự án này đang ở chế độ chỉ đọc.";
      case "UNKNOWN_REFERENCE_KEY": return "Khóa tham chiếu logic này chưa nằm trong danh sách cho phép.";
      case "CRS_CORRECTION_UNVERIFIED": return "Bằng chứng độc lập chưa xác minh được khai báo đề xuất.";
      case "NO_CHANGES": return "Không có thay đổi thực chất nào được cung cấp.";
      default: return "Không thể hoàn tất thao tác không gian.";
    }
  }
  if (error instanceof ApiError && [401, 403, 404].includes(error.status)) {
    return "Quyền hiện tại không cho phép thực hiện thao tác này.";
  }
  return "Không thể hoàn tất thao tác không gian.";
}

function CRSDescription({ label, crs }: {
  label: string;
  crs: SpatialSource["source_crs"];
}) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{crs ? <>
        EPSG:{crs.code} — {crs.name} ({crs.disposition})<br />
        Thứ tự trục gốc: {crs.native_axes.join(", ")}; thứ tự trong ứng dụng: X {crs.x_axis}, Y {crs.y_axis} ({crs.unit})
      </> : "Không xác định"}</dd>
    </div>
  );
}

function SourceMetadata({ source }: { source: SpatialSource }) {
  const extent = source.extent;
  return (
    <section aria-label="Thông tin nguồn dữ liệu" style={{ overflowWrap: "anywhere" }}>
      <h2>{source.name}</h2>
      <dl>
        <dt>Mã nguồn dữ liệu</dt><dd>{source.id}</dd>
        <dt>Trạng thái kiểm tra</dt><dd>{stateLabel(source.validation_state)}</dd>
        {source.validation_reason ? <>
          <dt>Lý do</dt><dd>{source.validation_reason.code}: {source.validation_reason.message}</dd>
        </> : null}
        <CRSDescription label="CRS khai báo" crs={source.source_crs} />
        <CRSDescription label="CRS gốc đang lưu" crs={source.stored_crs} />
        <dt>Số đối tượng</dt><dd>{source.feature_count_state === "known" ? source.feature_count : "Không khả dụng"}</dd>
        <dt>Phạm vi gốc</dt>
        <dd>{extent ? <>
          ({extent.min_x}, {extent.min_y}) – ({extent.max_x}, {extent.max_y})<br />
          EPSG:{extent.coordinate_reference.code}, {extent.coordinate_reference.unit}, X/Y
        </> : source.extent_state === "empty" ? "Nguồn rỗng (không có phạm vi)" : "Không khả dụng"}</dd>
        <dt>Lược đồ</dt><dd>{stateLabel(source.schema.state)} — {source.schema.geometry_family}, {source.schema.dimensions}</dd>
        <dt>Khóa tham chiếu</dt><dd>{source.provenance.reference_key}</dd>
        <dt>Phiên bản dữ liệu mẫu</dt><dd>{source.provenance.fixture_version}</dd>
        <dt>Mã băm manifest</dt><dd><code>{source.provenance.manifest_digest}</code></dd>
        <dt>Phiên bản danh mục</dt><dd>{source.catalog_revision}</dd>
        <dt>Phiên bản nội dung</dt><dd><code>{source.content_revision}</code></dd>
        <dt>Phiên bản lược đồ</dt><dd><code>{source.schema_revision}</code></dd>
      </dl>
      <h3>Trường dữ liệu</h3>
      {source.schema.state === "unavailable" ? <p>Không có lược đồ dữ liệu.</p> :
      source.schema.fields.length === 0 ? <p>Không có trường dữ liệu.</p> :
        <div style={{ overflowX: "auto" }}>
          <table>
            <thead><tr><th>ID</th><th>Tên</th><th>Kiểu</th><th>Cho phép rỗng</th></tr></thead>
            <tbody>{source.schema.fields.map(field => (
              <tr key={field.field_id}>
                <td>{field.field_id}</td><td>{field.name}</td><td>{field.type}</td>
                <td>{field.nullable ? "Có" : "Không"}</td>
              </tr>
            ))}</tbody>
          </table>
        </div>
      }
    </section>
  );
}


type LayerCommand =
  | { kind: "create"; sourceId: string; name: string }
  | { kind: "rename"; layer: SpatialLayer; name: string }
  | { kind: "retire"; layer: SpatialLayer };

function LayerPanel({ organizationId, projectId, accountId, source, writable }: {
  organizationId: string; projectId: string; accountId: string;
  source: SpatialSource | undefined; writable: boolean;
}) {
  const queryClient = useQueryClient();
  const prefix = ["spatial-catalog", accountId, organizationId, projectId, "layers"];
  const changesPrefix = historyPrefix({ accountId, organizationId, projectId });
  const [cursor, setCursor] = useState<string | undefined>();
  const [history, setHistory] = useState<(string | undefined)[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [rename, setRename] = useState("");
  const [confirmRetire, setConfirmRetire] = useState(false);
  const listQuery = useQuery({
    queryKey: [...prefix, "list", cursor ?? null],
    queryFn: () => listSpatialLayers(apiClient, organizationId, projectId, {
      limit: 20, ...(cursor === undefined ? {} : { after_id: cursor })
    }),
    ...protectedQueryOptions
  });
  const layers = currentProtectedData(listQuery);
  const detailQuery = useQuery({
    queryKey: [...prefix, "detail", selected],
    queryFn: () => getSpatialLayer(apiClient, organizationId, projectId, selected ?? ""),
    enabled: selected !== null && layers !== undefined,
    ...protectedQueryOptions
  });
  const detail = currentProtectedData(detailQuery);
  const command = useMutation<SpatialLayerMutation, Error, LayerCommand>({
    mutationFn: async input => {
      await loadCsrfToken(apiClient);
      switch (input.kind) {
        case "create": return createSpatialLayer(apiClient, organizationId, projectId, {
          source_id: input.sourceId, name: input.name
        });
        case "rename": return renameSpatialLayer(apiClient, organizationId, projectId, input.layer.id, {
          expected_revision: input.layer.layer_revision, name: input.name
        });
        case "retire": return retireSpatialLayer(apiClient, organizationId, projectId, input.layer.id, {
          expected_revision: input.layer.layer_revision
        });
      }
    },
    onSuccess: async (_result, input) => {
      if (input.kind === "create") {
        setName(""); setCursor(undefined); setHistory([]);
      }
      if (input.kind === "retire") setSelected(null);
      setRename(""); setConfirmRetire(false);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: prefix }),
        queryClient.invalidateQueries({ queryKey: changesPrefix })
      ]);
    },
    onError: async (error) => {
      setConfirmRetire(false);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: prefix }),
        queryClient.invalidateQueries({ queryKey: changesPrefix })
      ]);
      // Preserve an actionable stale/no-op error across a local re-read;
      // current-access failures also revalidate the parent gate.
      if ((error instanceof ApiError && [401, 403, 404].includes(error.status)) ||
          (error instanceof SpatialApiError && error.code === "PROJECT_NOT_WRITABLE")) {
        await queryClient.invalidateQueries({
          queryKey: ["spatial-project", accountId, organizationId, projectId]
        });
      }
    }
  });
  const editable = writable && !command.isPending && layers !== undefined;
  function choose(id: string | null) {
    setSelected(id); setRename(""); setConfirmRetire(false); command.reset();
  }
  return (
    <section aria-label="Danh mục lớp dữ liệu" style={{ borderTop: "1px solid #aaa", marginTop: 32, paddingTop: 16 }}>
      <h2>Lớp dữ liệu</h2>
      <p>Nhiều lớp có thể cùng tham chiếu một nguồn đã đăng ký mà không cần sao chép dữ liệu đối tượng.</p>
      {writable ? (
        <form onSubmit={event => {
          event.preventDefault();
          if (editable && source?.validation_state === "ready" && name.trim()) {
            command.mutate({ kind: "create", sourceId: source.id, name: name.trim() });
          }
        }}>
          <fieldset disabled={!editable || source?.validation_state !== "ready"}>
            <legend>Tạo lớp từ nguồn đang chọn</legend>
            <p>{source ? source.name + " — " + source.validation_state : "Hãy chọn một nguồn ở trạng thái sẵn sàng phía trên."}</p>
            <label>Tên lớp{" "}
              <input value={name} onChange={event => setName(event.target.value)} required maxLength={200} />
            </label>{" "}
            <button type="submit" disabled={!name.trim()}>Tạo lớp mới</button>
          </fieldset>
        </form>
      ) : null}
      {command.error ? <p role="alert">{safeError(command.error)}</p> : null}
      {!layers ? <p role={listQuery.error ? "alert" : "status"}>
        {listQuery.error ? "Không thể tải quyền truy cập lớp hiện tại." : "Đang kiểm tra quyền truy cập lớp…"}
      </p> : <>
        {layers.items.length ? (
          <ul aria-label="Các lớp đã đăng ký">
            {layers.items.map(layer => (
              <li key={layer.id}>
                <button type="button" onClick={() => choose(layer.id)}
                  disabled={command.isPending} aria-pressed={selected === layer.id}>
                  {layer.name} — {layer.source.name} — phiên bản {layer.layer_revision}
                </button>
              </li>
            ))}
          </ul>
        ) : <p>Không có lớp đang hoạt động.</p>}
        <button type="button" disabled={command.isPending || history.length === 0} onClick={() => {
          setCursor(history[history.length - 1]); setHistory(history.slice(0, -1)); choose(null);
        }}>Trang lớp trước</button>{" "}
        <button type="button" disabled={command.isPending || !layers.next_after_id} onClick={() => {
          if (layers.next_after_id) {
            setHistory([...history, cursor]); setCursor(layers.next_after_id); choose(null);
          }
        }}>Trang lớp sau</button>
        {selected !== null ? !detail ? <p role={detailQuery.error ? "alert" : "status"}>
          {detailQuery.error ? "Không thể tải quyền truy cập lớp hiện tại." : "Đang kiểm tra thông tin lớp…"}
        </p> : (
          <section aria-label="Thông tin lớp dữ liệu">
            <h3>{detail.name}</h3>
            <dl style={{ overflowWrap: "anywhere" }}>
              <dt>Mã lớp</dt><dd>{detail.id}</dd>
              <dt>Phiên bản lớp</dt><dd>{detail.layer_revision}</dd>
              <dt>Nguồn dữ liệu</dt><dd>{detail.source.name} ({detail.source_id})</dd>
              <dt>Trạng thái nguồn</dt><dd>{stateLabel(detail.source.validation_state)}</dd>
            </dl>
            <h4>Khả năng</h4>
            <div style={{ overflowX: "auto" }}>
              <table>
                <thead><tr><th>Thao tác</th><th>Được hỗ trợ</th><th>Khả dụng</th><th>Quyền yêu cầu</th><th>Được cấp quyền</th></tr></thead>
                <tbody>{detail.capabilities.map(capability => (
                  <tr key={capability.name}>
                    <td>{capability.name}</td><td>{capability.technical_supported ? "Có" : "Không"}</td>
                    <td>{capability.available ? "Có" : "Không"}</td><td>{capability.required_permission ?? "Không yêu cầu"}</td>
                    <td>{capability.actor_authorized ? "Có" : "Không"}</td>
                  </tr>
                ))}</tbody>
              </table>
            </div>
            <details>
              <summary>Lược đồ, CRS và phiên bản hiện tại của nguồn</summary>
              <SourceMetadata source={detail.source} />
            </details>
            {writable ? <fieldset disabled={!editable}>
              <legend>Điều khiển thông tin lớp</legend>
              <form onSubmit={event => {
                event.preventDefault();
                if (editable && rename.trim()) command.mutate({ kind: "rename", layer: detail, name: rename.trim() });
              }}>
                <label>New Tên lớp{" "}
                  <input value={rename} onChange={event => setRename(event.target.value)} required maxLength={200} />
                </label>{" "}
                <button type="submit" disabled={!rename.trim()}>Đổi tên lớp</button>
              </form>
              {!confirmRetire ? <button type="button" onClick={() => setConfirmRetire(true)}>Ngừng sử dụng lớp</button> : <>
                <p>Ngừng sử dụng sẽ ẩn lớp này. Nguồn dữ liệu và các lớp khác không bị thay đổi.</p>
                <button type="button" onClick={() => command.mutate({ kind: "retire", layer: detail })}>Xác nhận ngừng sử dụng lớp</button>{" "}
                <button type="button" onClick={() => setConfirmRetire(false)}>Hủy</button>
              </>}
            </fieldset> : null}
          </section>
        ) : null}
      </>}
      {selected !== null ? (
        <FeatureInspector key={selected} accountId={accountId} organizationId={organizationId}
          projectId={projectId} layerId={selected} layer={layers ? detail : undefined}
          reloadMetadata={async () => { await detailQuery.refetch(); }} />
      ) : null}
    </section>
  );
}

type CatalogCommand =
  | { kind: "register"; reference_key: string; name: string }
  | { kind: "rename"; source: SpatialSource; name: string }
  | { kind: "correct"; source: SpatialSource; declaration: SpatialDeclaredCrs }
  | { kind: "retire"; source: SpatialSource };

function CatalogView({ organizationId, projectId, accountId }: {
  organizationId: string; projectId: string; accountId: string;
}) {
  const queryClient = useQueryClient();
  const [afterId, setAfterId] = useState<string | undefined>();
  const [history, setHistory] = useState<(string | undefined)[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [referenceKey, setReferenceKey] = useState("");
  const [newName, setNewName] = useState("");
  const [rename, setRename] = useState("");
  const [crs, setCrs] = useState("4326");
  const [confirmRetire, setConfirmRetire] = useState(false);
  const queryPrefix = ["spatial-catalog", accountId, organizationId, projectId];
  const projectQuery = useQuery({
    queryKey: ["spatial-project", accountId, organizationId, projectId],
    queryFn: () => getProject(apiClient, organizationId, projectId),
    ...protectedQueryOptions
  });
  const project = currentProtectedData(projectQuery);
  const historyScope = { accountId, organizationId, projectId };
  const hasProjectAccess = canReadHistory(historyScope, project);
  const listQuery = useQuery({
    queryKey: [...queryPrefix, "list", afterId ?? null],
    queryFn: () => listSpatialSources(apiClient, organizationId, projectId, {
      limit: 20, ...(afterId === undefined ? {} : { after_id: afterId })
    }),
    ...protectedQueryOptions
  });
  const catalog = currentProtectedData(listQuery);
  const detailQuery = useQuery({
    queryKey: [...queryPrefix, "detail", selected],
    queryFn: () => getSpatialSource(apiClient, organizationId, projectId, selected ?? ""),
    enabled: selected !== null && hasProjectAccess && catalog !== undefined,
    ...protectedQueryOptions
  });
  const detail = currentProtectedData(detailQuery);
  const refresh = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryPrefix }),
      queryClient.invalidateQueries({ queryKey: ["spatial-project", accountId, organizationId, projectId] })
    ]);
  };
  const command = useMutation<SpatialSourceMutation, Error, CatalogCommand>({
    mutationFn: async (input) => {
      await loadCsrfToken(apiClient);
      switch (input.kind) {
        case "register": return registerSpatialSource(apiClient, organizationId, projectId, {
          reference_key: input.reference_key, name: input.name
        });
        case "rename": return updateSpatialSource(apiClient, organizationId, projectId, input.source.id, {
          expected_revision: input.source.catalog_revision, name: input.name
        });
        case "correct": return updateSpatialSource(apiClient, organizationId, projectId, input.source.id, {
          expected_revision: input.source.catalog_revision, declared_crs: input.declaration
        });
        case "retire": return retireSpatialSource(apiClient, organizationId, projectId, input.source.id, {
          expected_revision: input.source.catalog_revision
        });
      }
    },
    onSuccess: async (_result, input) => {
      if (input.kind === "register") {
        setReferenceKey(""); setNewName(""); setAfterId(undefined); setHistory([]);
      }
      if (input.kind === "retire") setSelected(null);
      setRename(""); setConfirmRetire(false);
      await refresh();
    },
    onError: async () => {
      setConfirmRetire(false);
      await refresh();
    }
  });
  function selectSource(id: string | null) {
    setSelected(id); setRename(""); setConfirmRetire(false); command.reset();
  }
  function register(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (referenceKey && newName.trim() && project?.status === "active") {
      command.mutate({ kind: "register", reference_key: referenceKey, name: newName.trim() });
    }
  }
  const writable = project?.status === "active" && !command.isPending;

  return (
    <main style={{ maxWidth: 1100, margin: "32px auto", padding: "0 16px", fontFamily: "system-ui, sans-serif" }}>
      <p><Link to={"/organizations/" + encodeURIComponent(organizationId) + "/projects"}>← Danh sách Dự án</Link></p>
      <h1>Danh mục nguồn dữ liệu không gian</h1>
      <p>Đăng ký và kiểm tra các nguồn tham chiếu tin cậy.</p>
      {!project || !hasProjectAccess || !catalog ? (
        <p role={projectQuery.error || listQuery.error || (project && !hasProjectAccess) ? "alert" : "status"}>
          {project && !hasProjectAccess ? "Phạm vi dự án hiện tại không khả dụng." :
            projectQuery.error || listQuery.error ? "Không thể tải quyền truy cập dự án hiện tại." : "Đang kiểm tra quyền truy cập dự án…"}
        </p>
      ) : (
        <>
          <p><strong>{project.name}</strong> · {stateLabel(project.status)}</p>
          {project.status === "archived" ? <p>Dự án đã lưu trữ — chỉ đọc.</p> : null}
          {project.status === "active" ? (
            <form onSubmit={register}>
              <fieldset disabled={!writable}>
                <legend>Đăng ký nguồn tham chiếu</legend>
                <label>Khóa tham chiếu logic{" "}
                  <input value={referenceKey} onChange={event => setReferenceKey(event.target.value)}
                    list="reference-fixtures" required pattern="[a-z0-9_]+" maxLength={100} />
                </label>
                <datalist id="reference-fixtures">
                  {fixtureSuggestions.map(key => <option key={key} value={key} />)}
                </datalist>
                {" "}<label>Tên nguồn dữ liệu{" "}
                  <input value={newName} onChange={event => setNewName(event.target.value)}
                    required maxLength={200} />
                </label>
                {" "}<button type="submit" disabled={!newName.trim()}>Đăng ký</button>
              </fieldset>
            </form>
          ) : null}
          {command.error ? <p role="alert">{safeError(command.error)}</p> : null}
          <h2>Nguồn dữ liệu đã đăng ký</h2>
          {catalog.items.length === 0 ? <p>Không có nguồn đang hoạt động.</p> : (
            <ul aria-label="Nguồn dữ liệu đã đăng ký">
              {catalog.items.map(source => (
                <li key={source.id}>
                  <button type="button" onClick={() => selectSource(source.id)}
                    disabled={command.isPending} aria-pressed={selected === source.id}>
                    {source.name} — {stateLabel(source.validation_state)}
                  </button>
                </li>
              ))}
            </ul>
          )}
          <div>
            <button type="button" disabled={command.isPending || history.length === 0} onClick={() => {
              setAfterId(history[history.length - 1]); setHistory(history.slice(0, -1)); selectSource(null);
            }}>Trang trước</button>{" "}
            <button type="button" disabled={command.isPending || !catalog.next_after_id} onClick={() => {
              if (catalog.next_after_id) {
                setHistory([...history, afterId]); setAfterId(catalog.next_after_id); selectSource(null);
              }
            }}>Trang sau</button>
          </div>
          {selected !== null ? !detail ? <p role={detailQuery.error ? "alert" : "status"}>
            {detailQuery.error ? "Không thể tải quyền truy cập nguồn hiện tại." : "Đang kiểm tra thông tin nguồn…"}
          </p> : <>
            <SourceMetadata source={detail} />
            {project.status === "active" ? (
              <fieldset disabled={!writable}>
                <legend>Điều khiển thông tin nguồn</legend>
                <form onSubmit={event => {
                  event.preventDefault();
                  if (rename.trim()) command.mutate({ kind: "rename", source: detail, name: rename.trim() });
                }}>
                  <label>Tên nguồn mới{" "}
                    <input value={rename} onChange={event => setRename(event.target.value)} maxLength={200} required />
                  </label>{" "}
                  <button type="submit" disabled={!rename.trim()}>Đổi tên</button>
                </form>
                {detail.validation_state !== "ready" ? (
                  <form onSubmit={event => {
                    event.preventDefault();
                    command.mutate({
                      kind: "correct", source: detail,
                      declaration: { authority: "EPSG", code: Number(crs), coordinate_order: "xy" }
                    });
                  }}>
                    <label>Sửa CRS khai báo (cần bằng chứng độc lập){" "}
                      <select value={crs} onChange={event => setCrs(event.target.value)}>
                        {[4326, 3857, 4756, 5899].map(code => <option key={code} value={code}>EPSG:{code}</option>)}
                      </select>
                    </label>{" "}<button type="submit">Xác minh và sửa</button>
                  </form>
                ) : null}
                {!confirmRetire ? <button type="button" onClick={() => setConfirmRetire(true)}>Ngừng sử dụng nguồn</button> :
                  <div>
                    <p>Ngừng sử dụng sẽ ẩn nguồn này khỏi truy cập thông thường. Các lớp đang hoạt động phải được ngừng trước.</p>
                    <button type="button" onClick={() => command.mutate({ kind: "retire", source: detail })}>Xác nhận ngừng sử dụng</button>{" "}
                    <button type="button" onClick={() => setConfirmRetire(false)}>Hủy</button>
                  </div>
                }
              </fieldset>
            ) : null}
          </> : null}
          <LayerPanel organizationId={organizationId} projectId={projectId}
            accountId={accountId} source={detail} writable={writable} />
          <CrsRegistryPanel scope={historyScope} />
          <CatalogHistoryPanel scope={historyScope} />
        </>
      )}
    </main>
  );
}

export function SpatialCatalogPage() {
  const { organizationId, projectId } = useParams();
  if (!organizationId || !projectId) return <p role="alert">Thiếu ngữ cảnh dự án.</p>;
  return (
    <FreshAccount key={organizationId + ":" + projectId}>
      {account => <CatalogView
        key={account.id + ":" + organizationId + ":" + projectId}
        organizationId={organizationId} projectId={projectId} accountId={account.id}
      />}
    </FreshAccount>
  );
}
