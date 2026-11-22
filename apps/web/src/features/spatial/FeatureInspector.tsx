import {
  ApiError, listSpatialCrsProfiles, getSpatialFeature, readSpatialFeatures, SpatialApiError,
  type SpatialFeature, type SpatialLayer, type SpatialSource
} from "@openvietgis/client-sdk";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { apiClient } from "../../app/api";
import { currentProtectedData, protectedQueryOptions } from "../../app/protected-query";
import { availableResponseCrs, crsRegistryKey, crsRegistryMatchesScope } from "./crs-profiles";
import {
  canReadFeatures, createTraversal, FEATURE_PAGE_SIZE,
  featureDetailKey, featureDetailNeedsRevalidation, featurePageKey, featureResponseMatchesScope, formatFeatureValue,
  isStaleFeaturePin, nextFeaturePage, previousFeaturePage, reconcileTraversal,
  restartFeatureTraversal, selectFeature, type FeatureReadScope, type FeatureTraversal
} from "./feature-inspector";

function readError(error: unknown): string {
  if (error instanceof SpatialApiError && error.code === "SOURCE_NOT_READY") {
    return "Nguồn dữ liệu không còn sẵn sàng để đọc đối tượng.";
  }
  if (error instanceof ApiError && [401, 403, 404].includes(error.status)) {
    return "Quyền hiện tại không cho phép đọc đối tượng này.";
  }
  if (error instanceof SpatialApiError && error.code === "TRANSFORMATION_OUT_OF_AREA") {
    return "Hình học nằm ngoài khu vực đã xác minh cho phép chuyển đổi tọa độ đang chọn. Hãy chọn CRS gốc hoặc phép chuyển đổi khác được hỗ trợ.";
  }
  return "Không thể tải dữ liệu đối tượng hiện tại.";
}

function FeatureDetail({
  feature, fields
}: { feature: SpatialFeature; fields: SpatialSource["schema"]["fields"] }) {
  const geometry = feature.geometry;
  return (
    <section aria-label="Chi tiết đối tượng đã chọn">
      <h4>Đối tượng {feature.feature_id}</h4>
      <div style={{ overflowX: "auto" }}>
        <table>
          <thead><tr><th>Mã trường</th><th>Tên</th><th>Kiểu</th><th>Giá trị</th></tr></thead>
          <tbody>{fields.map(field => (
            <tr key={field.field_id}>
              <th scope="row">{field.field_id}</th><td>{field.name}</td><td>{field.type}</td>
              <td><code style={{ overflowWrap: "anywhere" }}>{formatFeatureValue(feature.attributes[field.field_id])}</code></td>
            </tr>
          ))}</tbody>
        </table>
      </div>
      <p>Trạng thái hình học: {feature.geometry_state}</p>
      {geometry ? (
        <details>
          <summary>{geometry.type} — hình học phản hồi</summary>
          <p>
            EPSG:{geometry.coordinate_reference.code}, X {geometry.coordinate_reference.x_axis},
            {" "}Y {geometry.coordinate_reference.y_axis} ({geometry.coordinate_reference.unit}).
            Tọa độ X/Y tường minh; không phải GeoJSON.
          </p>
          <pre style={{ overflow: "auto", maxHeight: 240 }}>{JSON.stringify(geometry.coordinates, null, 2)}</pre>
        </details>
      ) : null}
    </section>
  );
}

function CurrentFeatureInspector({
  scope, fields, state, setTraversal, reloadMetadata
}: {
  scope: FeatureReadScope; fields: SpatialSource["schema"]["fields"];
  state: FeatureTraversal; setTraversal: (state: FeatureTraversal) => void;
  reloadMetadata: (clearSelection?: boolean) => void;
}) {
  const [detailBlocked, setDetailBlocked] = useState(false);
  const options = {
    content_revision: scope.contentRevision,
    schema_revision: scope.schemaRevision,
    include_geometry: true,
    ...(scope.responseCrs === undefined ? {} : { response_crs: scope.responseCrs })
  };
  const pageQuery = useQuery({
    queryKey: featurePageKey(scope, state.cursor),
    queryFn: () => readSpatialFeatures(apiClient, scope.organizationId, scope.projectId, scope.layerId, {
      ...options, limit: FEATURE_PAGE_SIZE,
      ...(state.cursor === null ? {} : { after_feature_id: state.cursor })
    }),
    enabled: !detailBlocked,
    ...protectedQueryOptions
  });
  const detailQuery = useQuery({
    queryKey: featureDetailKey(scope, state.selected),
    queryFn: () => getSpatialFeature(apiClient, scope.organizationId, scope.projectId,
      scope.layerId, state.selected ?? "", options),
    enabled: state.selected !== null && !detailBlocked,
    ...protectedQueryOptions
  });

  const blocked = featureDetailNeedsRevalidation(detailBlocked, detailQuery.error);
  if (blocked !== detailBlocked) setDetailBlocked(blocked);
  if (blocked) {
    return <>
      <p role="alert">Đối tượng hoặc lớp đã chọn không còn khả dụng. Hãy tải lại thông tin lớp trước khi xem dữ liệu.</p>
      <button type="button" onClick={() => reloadMetadata(true)}>Xác thực lại lớp và tải lại</button>
    </>;
  }
  const stale = isStaleFeaturePin(pageQuery.error) || isStaleFeaturePin(detailQuery.error);
  // A stale response in either read invalidates the whole pinned view: never
  // show an older cached page or detail while retrying the other.
  if (stale) {
    return <>
      <p role="alert">Nội dung hoặc lược đồ đã thay đổi. Hãy tải lại thông tin hiện tại và bắt đầu từ trang đầu.</p>
      <button type="button" onClick={() => reloadMetadata()}>Tải lại phiên bản và bắt đầu lại</button>
    </>;
  }
  if (detailQuery.error instanceof ApiError && [401, 403].includes(detailQuery.error.status)) {
    return <p role="alert">{readError(detailQuery.error)}</p>;
  }
  const page = currentProtectedData(pageQuery);
  if (!page) {
    return <p role={pageQuery.error ? "alert" : "status"}>
      {pageQuery.error ? readError(pageQuery.error) : "Đang kiểm tra quyền truy cập trang đối tượng…"}
    </p>;
  }
  if (!featureResponseMatchesScope(scope, page.context)) {
    return <p role="alert">Trang đối tượng không khớp với phiên bản và phạm vi hiện tại.</p>;
  }

  const detail = currentProtectedData(detailQuery);
  const matchingDetail = detail && featureResponseMatchesScope(scope, detail.context) &&
    detail.feature.feature_id === state.selected;
  const crs = page.context.response_crs;
  return (
    <>
      <p style={{ overflowWrap: "anywhere" }}>
        {page.context.transformation ? "Đã chuyển đổi" : "Gốc"} EPSG:{crs?.code} — X {crs?.x_axis}, Y {crs?.y_axis} ({crs?.unit}).<br />
        Phiên bản nội dung: <code>{scope.contentRevision}</code><br />
        Phiên bản lược đồ: <code>{scope.schemaRevision}</code>
      </p>
      {page.context.transformation ? <p role="status">
        Phép chuyển đổi: {page.context.transformation.operation_id}. Bằng chứng: {page.context.transformation.evidence_id}.
        {" "}Dung sai: {page.context.transformation.tolerance} {page.context.transformation.tolerance_unit}.
        {" "}{page.context.transformation.validated_area} Gốc storage is unchanged.
      </p> : null}
      <p>Giá trị hiển thị theo JSON chính xác: chuỗi có dấu ngoặc kép, null khác với chuỗi rỗng.</p>
      {page.items.length ? (
        <div style={{ overflowX: "auto" }}>
          <table aria-label="Pinned feature page">
            <thead><tr>
              <th>Mã đối tượng</th>
              {fields.map(field => (
                <th key={field.field_id}>{field.name}<br /><small>{field.field_id} — {field.type}</small></th>
              ))}
              <th>Trạng thái hình học</th>
            </tr></thead>
            <tbody>{page.items.map(feature => (
              <tr key={feature.feature_id}>
                <th scope="row">
                  <button type="button" aria-pressed={state.selected === feature.feature_id}
                    onClick={() => setTraversal(selectFeature(state, feature.feature_id))}>
                    {feature.feature_id}
                  </button>
                </th>
                {fields.map(field => (
                  <td key={field.field_id}><code>{formatFeatureValue(feature.attributes[field.field_id])}</code></td>
                ))}
                <td>{feature.geometry_state}</td>
              </tr>
            ))}</tbody>
          </table>
        </div>
      ) : <p>Không có đối tượng nào trên trang này.</p>}
      <p role="status">Trang {state.history.length + 1} — {page.items.length} đối tượng.</p>
      <button type="button" disabled={!state.history.length}
        onClick={() => setTraversal(previousFeaturePage(state))}>Trang đối tượng trước</button>{" "}
      <button type="button" disabled={page.next_after_feature_id === null}
        onClick={() => setTraversal(nextFeaturePage(state, page.next_after_feature_id))}>Trang đối tượng sau</button>
      {state.selected !== null ? (
        <section aria-label="Vùng chọn đối tượng">
          <p>Đối tượng đang chọn: <code>{state.selected}</code>{" "}
            <button type="button" onClick={() => setTraversal(selectFeature(state, null))}>Bỏ chọn</button>
          </p>
          {matchingDetail ? <FeatureDetail feature={detail.feature} fields={fields} /> :
            <p role={detailQuery.error || detail ? "alert" : "status"}>
              {detailQuery.error ? readError(detailQuery.error)
                  : detail ? "Chi tiết đối tượng không khớp với mã đang chọn và phiên bản hiện tại."
                    : "Đang kiểm tra quyền truy cập chi tiết đối tượng…"}
            </p>}
        </section>
      ) : <p>Chọn mã đối tượng để xem chính xác thuộc tính và hình học phản hồi.</p>}
    </>
  );
}

export function FeatureInspector({
  accountId, organizationId, projectId, layerId, layer, reloadMetadata
}: {
  accountId: string; organizationId: string; projectId: string; layerId: string;
  layer: SpatialLayer | undefined; reloadMetadata: () => Promise<void>;
}) {
  const [traversal, setTraversal] = useState(createTraversal);
  const [reloading, setReloading] = useState(false);
  const [targetCrs, setTargetCrs] = useState<number | undefined>();
  const registryScope = { accountId, organizationId, projectId };
  const registryQuery = useQuery({ queryKey: crsRegistryKey(registryScope),
    queryFn: () => listSpatialCrsProfiles(apiClient, organizationId, projectId),
    enabled: Boolean(layer), ...protectedQueryOptions });
  const registry = currentProtectedData(registryQuery);
  // The inspector stays mounted while Layer metadata is revalidated so the
  // explicit selection survives stale retries, but no old data stays visible.
  if (!layer || reloading) return null;
  if (layer.id !== layerId || layer.organization_id !== organizationId ||
      layer.project_id !== projectId || layer.source_id !== layer.source.id ||
      layer.source.organization_id !== organizationId || layer.source.project_id !== projectId) {
    return <p role="alert">Phạm vi lớp hiện tại không khớp với lớp được yêu cầu.</p>;
  }
  if (!canReadFeatures(layer) || !layer.source.stored_crs) {
    return <p role="status">Không thể kiểm tra đối tượng của lớp này.</p>;
  }

  if (!registry || !crsRegistryMatchesScope(registryScope, registry)) {
    return <p role={registryQuery.error ? "alert" : "status"}>{registryQuery.error
      ? "Không thể xác minh quyền truy cập cấu hình CRS hiện tại." : "Đang kiểm tra cấu hình CRS hiện tại…"}</p>;
  }
  const nativeCrs = layer.source.stored_crs.code;
  const responseOptions = availableResponseCrs(registry, nativeCrs);
  const responseCrs = targetCrs !== undefined && responseOptions.includes(targetCrs) ? targetCrs : nativeCrs;
  const transformation = responseCrs === nativeCrs ? undefined : registry.transformations.find(edge =>
    edge.source_code === nativeCrs && edge.target_code === responseCrs);
  const scope: FeatureReadScope = {
    accountId, organizationId, projectId, layerId, sourceId: layer.source_id,
    contentRevision: layer.source.content_revision,
    schemaRevision: layer.source.schema_revision,
    nativeCrs, responseCrs, ...(transformation ? { transformation } : {})
  };
  const state = reconcileTraversal(traversal, scope);
  // Reconcile before issuing a read, never sending an old cursor under a new pin.
  if (state !== traversal) setTraversal(state);

  return (
    <section aria-label="Kiểm tra đối tượng" style={{ borderTop: "1px solid #aaa", paddingTop: 16, marginTop: 16 }}>
      <h3>Kiểm tra đối tượng</h3>
      <label>Hệ tọa độ phản hồi{" "}
        <select value={responseCrs} onChange={event => setTargetCrs(Number(event.target.value))}>
          {responseOptions.map(code => <option key={code} value={code}>EPSG:{code}{code === nativeCrs ? " (gốc)" : " (phép chuyển đổi đã duyệt)"}</option>)}
        </select>
      </label>
      <CurrentFeatureInspector key={[scope.responseCrs, scope.contentRevision, scope.schemaRevision].join(":")} scope={scope} fields={layer.source.schema.fields}
        state={state} setTraversal={setTraversal} reloadMetadata={clearSelection => {
          const restarted = restartFeatureTraversal(state);
          setTraversal(clearSelection ? selectFeature(restarted, null) : restarted);
          setReloading(true);
          void reloadMetadata().finally(() => setReloading(false));
        }} />
    </section>
  );
}
