import { useEffect, useMemo, useState } from "react";
import {
  ApiError,
  changeQgisBindingDraftLifecycle,
  createQgisBindingDraft,
  listIntegrationBindingDrafts,
  listQgisBindingCandidates,
  loadCsrfToken,
  type ApiClient,
  type IntegrationBindingDraft,
  type QgisBindingCandidate
} from "@openvietgis/client-sdk";

interface Props {
  client: ApiClient;
  org: string;
  projectId: string;
}

/**
 * R027 dashboard is intentionally draft-only. No activation, fake pairing,
 * background worker, live connection badge, or "sync successful" button.
 */
export function QgisBindingManager({ client, org, projectId }: Props) {
  const [options, setOptions] = useState<QgisBindingCandidate[]>([]);
  const [bindings, setBindings] = useState<IntegrationBindingDraft[]>([]);
  const [selectedLayer, setSelectedLayer] = useState("");
  const [selectedFields, setSelectedFields] = useState<string[]>([]);
  const [datasetRef, setDatasetRef] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [forbidden, setForbidden] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError("");
    setForbidden(false);
    setNotice("");
    setSelectedLayer("");
    setSelectedFields([]);
    setDatasetRef("");
    setBindings([]);
    setOptions([]);
    void Promise.all([
      listQgisBindingCandidates(client, org, projectId),
      listIntegrationBindingDrafts(client, org, projectId)
    ]).then(([available, existing]) => {
      if (cancelled) return;
      setOptions(available.items);
      setBindings(existing.items);
    }).catch((reason: unknown) => {
      if (cancelled) return;
      if (reason instanceof ApiError && reason.status === 403) {
        setForbidden(true);
        return;
      }
      setError("Không thể tải cấu hình kết nối. Cần API R027 và quyền quản lý dự án.");
    }).finally(() => {
      if (!cancelled) setLoading(false);
    });
    return () => { cancelled = true; };
  }, [client, org, projectId]);

  const candidate = useMemo(
    () => options.find((item) => item.layer_id === selectedLayer) ?? null,
    [options, selectedLayer]
  );

  function selectLayer(id: string) {
    setSelectedLayer(id);
    const next = options.find((item) => item.layer_id === id);
    setSelectedFields(next ? next.allowed_field_ids.slice(0, 100) : []);
    setError("");
    setNotice("");
  }

  async function createDraft() {
    if (!candidate || selectedFields.length === 0 || saving) return;
    const name = datasetRef.trim();
    if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(name)) {
      setError("Mã bộ dữ liệu chỉ được dùng chữ, số, dấu chấm, gạch nối hoặc gạch dưới.");
      return;
    }
    setSaving(true);
    setError("");
    setNotice("");
    try {
      const csrf = await loadCsrfToken(client);
      const created = await createQgisBindingDraft(
        client, org, projectId,
        {
          adapter_key: "qgis",
          layer_id: candidate.layer_id,
          external_dataset_ref: name,
          direction: "bidirectional_manual",
          source_crs_code: candidate.stored_crs_code,
          target_crs_code: candidate.stored_crs_code,
          external_primary_key: "ovg_feature_id",
          allowed_field_ids: [...selectedFields].sort()
        },
        csrf
      );
      setBindings((prev) => [created, ...prev]);
      setDatasetRef("");
      setNotice("Đã tạo cấu hình nháp. Chưa kích hoạt hoặc đồng bộ dữ liệu.");
    } catch (reason: unknown) {
      if (reason instanceof ApiError && reason.status === 409) {
        setError("Lớp và mã bộ dữ liệu này đã có cấu hình chưa thu hồi.");
      } else if (reason instanceof ApiError && reason.status === 403) {
        setError("Không đủ quyền quản lý dự án hoặc phạm vi GIS.");
      } else {
        setError("Không thể lưu cấu hình. Kiểm tra CRS, mã trường hoặc phiên API.");
      }
    } finally {
      setSaving(false);
    }
  }

  async function changeState(binding: IntegrationBindingDraft, action: "pause" | "revoke") {
    if (saving) return;
    if (action === "revoke" && !window.confirm(
      "Thu hồi cấu hình này? Thao tác không xóa dữ liệu GIS nhưng không thể khôi phục bản nháp."
    )) return;
    setSaving(true);
    setError("");
    setNotice("");
    try {
      const csrf = await loadCsrfToken(client);
      const result = await changeQgisBindingDraftLifecycle(
        client, org, projectId, binding.id, action, binding.revision, csrf
      );
      setBindings((prev) => prev.map((item) => item.id === binding.id ? result : item));
      setNotice(action === "pause" ? "Đã tạm dừng cấu hình." : "Đã thu hồi cấu hình.");
    } catch (reason: unknown) {
      setError(reason instanceof ApiError && reason.status === 409
        ? "Phiên bản cấu hình đã thay đổi. Tải lại màn hình trước khi thao tác."
        : "Không thể cập nhật trạng thái; kiểm tra quyền dự án.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="integration-binding-manager" aria-label="Quản lý cấu hình QGIS R027">
      <header className="integration-binding-header">
        <h4>QGIS – Cấu hình liên thông hai chiều</h4>
        <span>R027 · Bản nháp · Chưa kết nối</span>
      </header>
      <p>
        Đăng ký lớp dữ liệu để chuẩn bị trao đổi với plugin QGIS 0.3.0.
        Đồng bộ hiện thực hiện <strong>thủ công trên QGIS Desktop</strong> qua
        GeoPackage và Changeset; một cấu hình ở đây không tự đồng bộ.
        {" "}<a
          href="https://github.com/nguyentrieu210/OpenVietGis/blob/feature/027-bidirectional-integration-sync/integrations/qgis_plugin/openvietgis_connector/R027-MANUAL-SYNC.md"
          target="_blank"
          rel="noopener noreferrer"
        >Hướng dẫn cài plugin QGIS thử nghiệm</a>.
      </p>
      {loading && <p role="status">Đang kiểm tra quyền và các lớp đủ điều kiện…</p>}
      {forbidden && <p role="status">Quản lý cấu hình dành cho tài khoản có quyền quản trị dự án và phạm vi GIS đầy đủ.</p>}
      {error && <p className="error" role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      {!loading && !forbidden && (
        <>
          <div className="integration-binding-form">
            <label>
              Lớp bản đồ
              <select
                aria-label="Chọn lớp kết nối QGIS"
                value={selectedLayer}
                onChange={(e) => selectLayer(e.target.value)}
                disabled={saving}
              >
                <option value="">Chọn lớp có CRS kiểm chứng và hình học 2D…</option>
                {options.map((item) => (
                  <option key={item.layer_id} value={item.layer_id}>
                    {item.layer_name} · EPSG:{item.stored_crs_code}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Mã bộ dữ liệu QGIS (không phải đường dẫn hoặc URL)
              <input
                type="text"
                value={datasetRef}
                maxLength={128}
                autoComplete="off"
                placeholder="vi_du_ban_do_dia_chinh_01"
                disabled={saving}
                onChange={(e) => setDatasetRef(e.target.value)}
              />
            </label>
          </div>
          {candidate && (
            <fieldset className="integration-binding-fields">
              <legend>Trường được phép ánh xạ ({selectedFields.length}/{candidate.allowed_field_ids.length})</legend>
              {candidate.allowed_field_ids.map((field) => (
                <label key={field}>
                  <input
                    type="checkbox"
                    checked={selectedFields.includes(field)}
                    disabled={saving}
                    onChange={(event) => {
                      setSelectedFields((previous) => event.target.checked
                        ? [...previous, field].slice(0, 100)
                        : previous.filter((value) => value !== field));
                    }}
                  />
                  {field}
                </label>
              ))}
            </fieldset>
          )}
          <button
            type="button"
            className="integration-binding-create"
            disabled={saving || !candidate || selectedFields.length === 0 || !datasetRef.trim()}
            onClick={() => { void createDraft(); }}
          >
            {saving ? "Đang xử lý…" : "Tạo cấu hình QGIS (chưa kích hoạt)"}
          </button>
          <h5>Cấu hình trong dự án ({bindings.length})</h5>
          {bindings.length === 0 ? (
            <p>Chưa có cấu hình. Có thể dùng nút Xuất GeoPackage của R026 ngay mà không cần cấu hình nháp.</p>
          ) : (
            <div className="integration-binding-list">
              {bindings.map((binding) => (
                <article key={binding.id}>
                  <div>
                    <strong>{binding.external_dataset_ref}</strong>
                    <small>
                      {options.find((item) => item.layer_id === binding.layer_id)?.layer_name ?? binding.layer_id}
                      {" · "}EPSG:{binding.source_crs_code} · {binding.allowed_field_ids.length} trường
                    </small>
                  </div>
                  <div className="integration-binding-actions">
                    <span>{binding.state === "draft" ? "Nháp" : binding.state === "paused" ? "Tạm dừng" : "Đã thu hồi"}</span>
                    {binding.state === "draft" && <button type="button" disabled={saving}
                      onClick={() => { void changeState(binding, "pause"); }}>Tạm dừng</button>}
                    {binding.state !== "revoked" && <button type="button" disabled={saving}
                      onClick={() => { void changeState(binding, "revoke"); }}>Thu hồi</button>}
                  </div>
                  <p>Chưa ghép nối QGIS Agent · Không tự ghi/xóa · Không có trạng thái kết nối trực tiếp.</p>
                </article>
              ))}
            </div>
          )}
        </>
      )}
    </section>
  );
}
