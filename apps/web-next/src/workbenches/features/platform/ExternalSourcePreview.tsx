import {
  ApiError,
  type ExternalConnection,
  type ExternalProviderPreview,
  type ExternalViewportRequest,
  loadCsrfToken,
  previewExternalProvider,
  previewExternalProviderViewport
} from "@openvietgis/client-sdk";
import { useState } from "react";
import { apiClient } from "../../app/api";
import { externalAvailabilityLabel } from "./platform-release-state";

interface Props {
  connection: ExternalConnection;
  organizationId: string;
  projectId: string;
}

interface ViewportDraft {
  minX: string;
  minY: string;
  maxX: string;
  maxY: string;
  width: string;
  height: string;
}

function draftFor(preview: ExternalProviderPreview): ViewportDraft {
  return {
    minX: String(preview.bbox[0]), minY: String(preview.bbox[1]),
    maxX: String(preview.bbox[2]), maxY: String(preview.bbox[3]),
    width: String(preview.width ?? 256), height: String(preview.height ?? 256)
  };
}

/** This is a fresh sample, not a promise of permanent availability. */
export function ExternalSourcePreview({ connection, organizationId, projectId }: Props) {
  const [preview, setPreview] = useState<ExternalProviderPreview | null>(null);
  const [pending, setPending] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [viewportPolicy, setViewportPolicy] = useState<{
    crs: number; bbox: [number, number, number, number]
  } | null>(null);
  const [draft, setDraft] = useState<ViewportDraft | null>(null);
  const supported = ["wms", "wfs_ogc_features", "wmts_xyz", "cog_geotiff"].includes(connection.protocol);

  if (connection.lifecycle === "revoked" || connection.availability === "revoked") {
    return <span> — {externalAvailabilityLabel(connection)}</span>;
  }
  if (!supported) return <span> — Chưa hỗ trợ xem mẫu · chỉ cấu hình</span>;

  const verify = async (viewport?: ExternalViewportRequest) => {
    setPending(true);
    setPreview(null); // Never keep previous pixels while revalidating.
    setFailure(null);
    if (!viewport) {
      setViewportPolicy(null);
      setDraft(null);
    }
    try {
      const csrf = await loadCsrfToken(apiClient);
      const response = viewport
        ? await previewExternalProviderViewport(
          apiClient, organizationId, projectId, connection.id, viewport, csrf
        )
        : await previewExternalProvider(apiClient, organizationId, projectId, connection.id, csrf);
      if (
        response.connection_id !== connection.id ||
        response.cache_generation !== connection.cache_generation ||
        response.availability !== "available" ||
        response.scope !== (viewport ? "operator_approved_viewport" : "operator_approved_sample")
      ) {
        setFailure("Nguồn thay đổi trong khi xác minh. Vui lòng thử lại.");
      } else {
        setPreview(response);
        if (response.sample?.kind === "cog_window_sample" && response.sample.allow_viewport) {
          setViewportPolicy({ crs: response.crs, bbox: response.sample.approved_bbox });
          setDraft(draftFor(response));
        }
      }
    } catch (error) {
      if (error instanceof ApiError && error.status === 502) {
        setFailure("Nguồn hiện không khả dụng hoặc cửa sổ vượt giới hạn đọc.");
      } else if (error instanceof ApiError && error.status === 403) {
        setFailure("Nguồn/cửa sổ chưa được quản trị viên phê duyệt hoặc thiếu quyền.");
      } else if (error instanceof ApiError && error.status === 422) {
        setFailure("Tọa độ, hệ quy chiếu hoặc kích thước cửa sổ không hợp lệ.");
      } else {
        setFailure("Không thể xác minh nguồn. Vui lòng thử lại.");
      }
    } finally {
      setPending(false);
    }
  };

  const readViewport = () => {
    if (!draft || !viewportPolicy) return;
    const values = [draft.minX, draft.minY, draft.maxX, draft.maxY, draft.width, draft.height];
    const bbox: ExternalViewportRequest["bbox"] = [
      Number(draft.minX), Number(draft.minY), Number(draft.maxX), Number(draft.maxY)
    ];
    const width = Number(draft.width);
    const height = Number(draft.height);
    const allowed = viewportPolicy.bbox;
    if (
      values.some(v => v.trim() === "" || !Number.isFinite(Number(v))) ||
      !Number.isInteger(width) || !Number.isInteger(height) ||
      width < 1 || height < 1 || width > 512 || height > 512 ||
      !(allowed[0] <= bbox[0] && bbox[0] < bbox[2] && bbox[2] <= allowed[2] &&
        allowed[1] <= bbox[1] && bbox[1] < bbox[3] && bbox[3] <= allowed[3])
    ) {
      setPreview(null);
      setFailure("Cửa sổ phải nằm trong vùng duyệt; kích thước từ 1 đến 512 pixel.");
      return;
    }
    void verify({ crs: viewportPolicy.crs, bbox, width, height });
  };

  return (
    <div>
      <span>{externalAvailabilityLabel(connection)}</span>{" "}
      <button type="button" className="ovg-btn ovg-btn-secondary" disabled={pending} onClick={() => void verify()}>
        {pending ? "Đang xác minh..." : "Xác minh & xem mẫu được phép"}
      </button>
      {failure && <p role="alert">{failure}</p>}
      {viewportPolicy && draft && (
        <fieldset disabled={pending}>
          <legend>Cửa sổ COG trong vùng đã duyệt (EPSG:{viewportPolicy.crs})</legend>
          <p>Giới hạn: {viewportPolicy.bbox.join(", ")}</p>
          {([
            ["minX", "Min X"], ["minY", "Min Y"], ["maxX", "Max X"], ["maxY", "Max Y"],
            ["width", "Rộng (pixel)"], ["height", "Cao (pixel)"]
          ] as const).map(([key, label]) => (
            <label key={key}>
              {label}{" "}
              <input
                type="number"
                step={key === "width" || key === "height" ? "1" : "any"}
                value={draft[key]}
                onChange={e => setDraft({ ...draft, [key]: e.target.value })}
              />
            </label>
          ))}
          <button type="button" className="ovg-btn ovg-btn-secondary" onClick={readViewport}>
            Đọc cửa sổ COG
          </button>
        </fieldset>
      )}
      {preview && (
        <section aria-label="Mẫu dữ liệu đã xác minh">
          <p><strong>Đã xác minh mẫu tại thời điểm yêu cầu</strong> — EPSG:{preview.crs}</p>
          <p>Vùng {preview.scope === "operator_approved_viewport" ? "cửa sổ" : "mẫu"} được duyệt: {preview.bbox.join(", ")}</p>
          {preview.image_png_base64 && preview.capabilities.render && (
            <img
              alt={`Ảnh raster ${preview.layer_identifier} tại vùng mẫu đã duyệt`}
              width={preview.width}
              height={preview.height}
              src={`data:image/png;base64,${preview.image_png_base64}`}
              style={{ maxWidth: "100%", height: "auto" }}
            />
          )}
          {preview.sample?.kind === "geotiff_range_sample" && (
            <p>
              Mẫu GeoTIFF range: {preview.sample.tile_count} tile, {preview.sample.band_count} kênh,
              kích thước pixel {preview.sample.pixel_size.join(" × ")};
              cấu hình này chỉ hỗ trợ mẫu nhỏ.
            </p>
          )}
          {preview.sample?.kind === "cog_window_sample" && (
            <p>
              COG: overview {preview.sample.overview_index}/{preview.sample.overview_count}
              {" "} (tỉ lệ {preview.sample.overview_scale}×), {preview.sample.tile_count} tile,
              {" "}{preview.sample.compression}; đã tải {preview.sample.range_bytes.toLocaleString()}
              {" "}/ {preview.sample.object_bytes.toLocaleString()} byte qua {preview.sample.range_requests} range.
              Cửa sổ pixel nguồn: {preview.sample.source_pixel_window.join(", ")}.
            </p>
          )}
          {(preview.sample?.kind === "xyz" || preview.sample?.kind === "wmts") && (
            <p>Tile {preview.sample.kind.toUpperCase()}: z={preview.sample.z}, x={preview.sample.x}, y={preview.sample.y}</p>
          )}
          {preview.feature_collection && preview.capabilities.feature_read && (
            <>
              <p>Đối tượng trong mẫu: {preview.feature_collection.features.length}</p>
              {preview.feature_result ? (
                <div role="status">
                  <p>Giới hạn yêu cầu: {preview.feature_result.request_limit} đối tượng.
                    {" "}Tổng khớp do nguồn báo: {preview.feature_result.provider_reported_matched ?? "chưa biết"}.</p>
                  {preview.feature_result.coverage === "partial" ? (
                    <p>Kết quả một phần — nhà cung cấp báo còn đối tượng chưa trả về.</p>
                  ) : preview.feature_result.coverage === "provider_reported_complete" ? (
                    <p>Nhà cung cấp báo đủ trong phạm vi mẫu yêu cầu; không xác nhận toàn bộ nguồn hoặc phiên bản bất biến.</p>
                  ) : (
                    <p>Chưa xác định độ đầy đủ của kết quả; không suy ra đã nhận hết dữ liệu.</p>
                  )}
                  {preview.feature_result.limit_reached && <p>Đã chạm giới hạn yêu cầu.</p>}
                  <p>Không hỗ trợ truy vấn trang tiếp theo trong hồ sơ mẫu này.</p>
                </div>
              ) : <p>Chưa xác định độ đầy đủ của mẫu.</p>}
              <details>
                <summary>Xem dữ liệu GeoJSON đã xác minh</summary>
                <pre style={{ maxHeight: 300, overflow: "auto" }}>
                  {JSON.stringify(preview.feature_collection, null, 2)}
                </pre>
              </details>
            </>
          )}
          <p>Ghi công: {preview.attribution} · Quyền sử dụng: {preview.license_text}</p>
          <p>Mẫu chỉ đọc, không bật chỉnh sửa, phân tích, tải xuất hay ngoại tuyến.</p>
          <button type="button" className="ovg-btn ovg-btn-secondary" onClick={() => {
            setPreview(null);
            setViewportPolicy(null);
            setDraft(null);
          }}>
            Đóng mẫu
          </button>
        </section>
      )}
    </div>
  );
}
