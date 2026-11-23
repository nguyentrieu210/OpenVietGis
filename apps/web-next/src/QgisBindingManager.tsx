import { useEffect, useMemo, useState } from "react";
import {
  ApiError,
  activateQgisBinding,
  changeQgisBindingDraftLifecycle,
  createQgisBindingDraft,
  issueBindingPairingToken,
  listBindingConflicts,
  listBindingPairingTokens,
  listIntegrationBindingDrafts,
  listQgisBindingCandidates,
  loadCsrfToken,
  resolveBindingConflict,
  revokeBindingPairingToken,
  type ApiClient,
  type BindingPairingToken,
  type IntegrationBindingDraft,
  type IssuedPairingTokenResponse,
  type QgisBindingCandidate,
  type SyncConflictRecord
} from "@openvietgis/client-sdk";

interface Props {
  client: ApiClient;
  org: string;
  projectId: string;
}

function formatJsonValue(value: unknown): string {
  if (value === null || value === undefined) return "—";
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function stateLabel(binding: IntegrationBindingDraft): string {
  if (binding.state === "active") {
    return binding.connected ? "Đang hoạt động · Đã ghép nối" : "Đang hoạt động";
  }
  if (binding.state === "paused") return "Tạm dừng";
  if (binding.state === "revoked") return "Đã thu hồi";
  return "Nháp · Chưa kết nối";
}

function conflictStatusLabel(status: SyncConflictRecord["status"]): string {
  if (status === "open") return "Chưa xử lý";
  if (status === "resolved_web") return "Đã chọn bản Web";
  if (status === "resolved_qgis") return "Đã chọn bản QGIS";
  if (status === "resolved_merged") return "Đã gộp tùy chỉnh";
  return "Đã bỏ qua";
}

export function QgisBindingManager({ client, org, projectId }: Props) {
  const [options, setOptions] = useState<QgisBindingCandidate[]>([]);
  const [bindings, setBindings] = useState<IntegrationBindingDraft[]>([]);
  const [loading, setLoading] = useState(true);
  const [forbidden, setForbidden] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [saving, setSaving] = useState(false);
  const [selectedLayer, setSelectedLayer] = useState("");
  const [selectedFields, setSelectedFields] = useState<string[]>([]);
  const [datasetRef, setDatasetRef] = useState("");

  // Pha A: Pairing Token state
  const [pairingOpenId, setPairingOpenId] = useState<string | null>(null);
  const [pairingTokens, setPairingTokens] = useState<Record<string, BindingPairingToken[]>>({});
  const [pairingLabel, setPairingLabel] = useState("QGIS Desktop");
  const [pairingDays, setPairingDays] = useState("30");
  const [issuedPairing, setIssuedPairing] = useState<IssuedPairingTokenResponse | null>(null);

  // Pha B: Conflict Resolution state
  const [conflictsOpenId, setConflictsOpenId] = useState<string | null>(null);
  const [conflictsByBinding, setConflictsByBinding] = useState<Record<string, SyncConflictRecord[]>>({});
  const [mergeDrafts, setMergeDrafts] = useState<Record<string, string>>({});

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
    setPairingOpenId(null);
    setConflictsOpenId(null);
    setIssuedPairing(null);
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

  async function copyText(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      setNotice(`Đã sao chép ${label}.`);
    } catch {
      setError(`Không thể tự sao chép ${label}. Hãy bôi đen và sao chép thủ công.`);
    }
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
          allowed_field_ids: selectedFields
        },
        csrf
      );
      setBindings((previous) => [created, ...previous]);
      setDatasetRef("");
      setNotice("Đã tạo cấu hình QGIS. Bấm 'Kích hoạt' hoặc 'Mã ghép nối QGIS' để kết nối trực tiếp.");
    } catch (reason: unknown) {
      if (reason instanceof ApiError && reason.status === 409) {
        setError("Cấu hình với mã bộ dữ liệu này đã tồn tại trên lớp đã chọn.");
      } else if (reason instanceof ApiError && reason.status === 422) {
        setError("Dữ liệu cấu hình không hợp lệ (kiểm tra lớp, CRS và trường đã chọn).");
      } else {
        setError("Không thể tạo cấu hình QGIS.");
      }
    } finally {
      setSaving(false);
    }
  }

  async function activate(binding: IntegrationBindingDraft) {
    if (saving) return;
    setSaving(true);
    setError("");
    setNotice("");
    try {
      const csrf = await loadCsrfToken(client);
      const updated = await activateQgisBinding(
        client, org, projectId, binding.id, binding.revision, csrf
      );
      setBindings((prev) => prev.map((item) => (item.id === updated.id ? updated : item)));
      setNotice(`Đã kích hoạt cấu hình '${binding.external_dataset_ref}'.`);
    } catch (reason: unknown) {
      setError(reason instanceof ApiError && reason.status === 409
        ? "Phiên bản cấu hình đã thay đổi; hãy tải lại."
        : "Không thể kích hoạt cấu hình.");
    } finally {
      setSaving(false);
    }
  }

  async function changeState(binding: IntegrationBindingDraft, action: "pause" | "revoke") {
    if (saving) return;
    setSaving(true);
    setError("");
    setNotice("");
    try {
      const csrf = await loadCsrfToken(client);
      const updated = await changeQgisBindingDraftLifecycle(
        client, org, projectId, binding.id, action, binding.revision, csrf
      );
      setBindings((previous) => previous.map((item) => (item.id === updated.id ? updated : item)));
      if (action === "revoke" && issuedPairing?.binding.id === binding.id) {
        setIssuedPairing(null);
      }
      setNotice(action === "pause"
        ? "Đã tạm dừng cấu hình."
        : "Đã thu hồi vĩnh viễn cấu hình và toàn bộ mã ghép nối liên quan.");
    } catch (reason: unknown) {
      if (reason instanceof ApiError && reason.status === 409) {
        setError("Phiên bản cấu hình đã thay đổi; hãy đóng mở lại hộp thoại.");
      } else {
        setError("Không thể cập nhật trạng thái cấu hình.");
      }
    } finally {
      setSaving(false);
    }
  }

  async function togglePairing(bindingId: string) {
    if (pairingOpenId === bindingId) {
      setPairingOpenId(null);
      return;
    }
    setPairingOpenId(bindingId);
    setError("");
    try {
      const res = await listBindingPairingTokens(client, org, projectId, bindingId);
      setPairingTokens((prev) => ({ ...prev, [bindingId]: res.items }));
    } catch {
      setError("Không thể tải danh sách mã ghép nối.");
    }
  }

  async function issuePairing(binding: IntegrationBindingDraft) {
    if (saving) return;
    setSaving(true);
    setError("");
    setNotice("");
    try {
      const csrf = await loadCsrfToken(client);
      const res = await issueBindingPairingToken(
        client,
        org,
        projectId,
        binding.id,
        { label: pairingLabel.trim() || "QGIS Desktop", expires_in_days: Number(pairingDays) || 30 },
        csrf
      );
      setIssuedPairing(res);
      setBindings((prev) => prev.map((item) => (item.id === res.binding.id ? res.binding : item)));
      setPairingTokens((prev) => ({
        ...prev,
        [binding.id]: [res.token, ...(prev[binding.id] ?? [])]
      }));
      setNotice("Đã tạo mã ghép nối QGIS & kích hoạt cấu hình. Hãy sao chép mã ngay (chỉ hiện 1 lần).");
    } catch {
      setError("Không thể tạo mã ghép nối QGIS (kiểm tra quyền quản trị tổ chức/dự án).");
    } finally {
      setSaving(false);
    }
  }

  async function revokeToken(bindingId: string, tokenId: string) {
    if (saving) return;
    setSaving(true);
    setError("");
    setNotice("");
    try {
      const csrf = await loadCsrfToken(client);
      const revoked = await revokeBindingPairingToken(client, org, projectId, bindingId, tokenId, csrf);
      setPairingTokens((prev) => ({
        ...prev,
        [bindingId]: (prev[bindingId] ?? []).map((t) => (t.id === revoked.id ? revoked : t))
      }));
      setNotice("Đã thu hồi mã ghép nối QGIS ngay lập tức.");
    } catch {
      setError("Không thể thu hồi mã ghép nối.");
    } finally {
      setSaving(false);
    }
  }

  async function toggleConflicts(bindingId: string) {
    if (conflictsOpenId === bindingId) {
      setConflictsOpenId(null);
      return;
    }
    setConflictsOpenId(bindingId);
    setError("");
    try {
      const res = await listBindingConflicts(client, org, projectId, bindingId);
      setConflictsByBinding((prev) => ({ ...prev, [bindingId]: res.items }));
    } catch {
      setError("Không thể tải danh sách xung đột.");
    }
  }

  async function resolveConflict(
    bindingId: string,
    conflict: SyncConflictRecord,
    decision: "keep_web" | "keep_qgis" | "merge" | "dismiss"
  ) {
    if (saving) return;
    let mergedValue: unknown = undefined;
    if (decision === "merge") {
      const raw = (mergeDrafts[conflict.id] ?? "").trim();
      if (!raw) {
        setError("Nhập giá trị gộp trước khi bấm 'Lưu giá trị gộp'.");
        return;
      }
      try {
        mergedValue = JSON.parse(raw);
      } catch {
        mergedValue = raw;
      }
    }
    setSaving(true);
    setError("");
    setNotice("");
    try {
      const csrf = await loadCsrfToken(client);
      const updated = await resolveBindingConflict(
        client,
        org,
        projectId,
        bindingId,
        conflict.id,
        decision === "merge" ? { decision, merged_value: mergedValue } : { decision },
        csrf
      );
      setConflictsByBinding((prev) => ({
        ...prev,
        [bindingId]: (prev[bindingId] ?? []).map((c) => (c.id === updated.id ? updated : c))
      }));
      setBindings((prev) => prev.map((b) => (
        b.id === bindingId
          ? { ...b, open_conflicts_count: Math.max(0, (b.open_conflicts_count ?? 1) - 1) }
          : b
      )));
      setNotice(`Đã xử lý xung đột đối tượng ${conflict.feature_id}: ${conflictStatusLabel(updated.status)}.`);
    } catch {
      setError("Không thể lưu quyết định xử lý xung đột.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="integration-binding-manager" aria-label="Quản lý cấu hình QGIS R027">
      <header className="integration-binding-header">
        <h4>QGIS – Liên thông hai chiều & Mã ghép nối (R027)</h4>
        <span>Pha A + B · Pairing & Xung đột</span>
      </header>
      <p>
        Tạo cấu hình, cấp <strong>Mã ghép nối QGIS</strong> trực tiếp (không cần vào Developer Portal) và xử lý xung đột giữa bản Web / bản QGIS ngay tại đây. Không tự ghi/xóa ngầm — mọi đồng bộ đều qua xem trước & R006 Changeset.
        {" "}<a
          href="/rewrite/downloads/openvietgis_connector-0.4.0.zip"
          download="openvietgis_connector-0.4.0.zip"
        >⬇ Tải Plugin QGIS 0.4.0 (.zip)</a>
        {" · "}<a
          href="https://github.com/nguyentrieu210/OpenVietGis/blob/main/integrations/qgis_plugin/openvietgis_connector/R027-MANUAL-SYNC.md"
          target="_blank"
          rel="noopener noreferrer"
        >Hướng dẫn cài & sử dụng (Tiếng Việt)</a>.
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
            {saving ? "Đang xử lý…" : "＋ Tạo cấu hình QGIS"}
          </button>
          <h5>Cấu hình trong dự án ({bindings.length})</h5>
          {bindings.length === 0 ? (
            <p>Chưa có cấu hình. Tạo một cấu hình phía trên rồi bấm "🔑 Mã ghép nối QGIS" để kết nối thẳng từ QGIS Desktop.</p>
          ) : (
            <div className="integration-binding-list">
              {bindings.map((binding) => {
                const openCount = binding.open_conflicts_count ?? 0;
                const tokens = pairingTokens[binding.id] ?? [];
                const conflicts = conflictsByBinding[binding.id] ?? [];
                return (
                  <article key={binding.id} className={binding.state === "active" ? "is-active-binding" : ""}>
                    <div>
                      <strong>{binding.external_dataset_ref}</strong>
                      <small>
                        {options.find((item) => item.layer_id === binding.layer_id)?.layer_name ?? binding.layer_id}
                        {" · "}EPSG:{binding.source_crs_code} · {binding.allowed_field_ids.length} trường
                      </small>
                    </div>
                    <div className="integration-binding-actions">
                      <span className={`binding-state-pill is-${binding.state}`}>{stateLabel(binding)}</span>
                      {(binding.state === "draft" || binding.state === "paused") && (
                        <button type="button" className="is-activate" disabled={saving}
                          onClick={() => { void activate(binding); }}>Kích hoạt</button>
                      )}
                      {binding.state !== "revoked" && (
                        <button type="button" className="is-pair" disabled={saving}
                          onClick={() => { void togglePairing(binding.id); }}>🔑 Mã ghép nối QGIS</button>
                      )}
                      <button type="button" className={openCount > 0 ? "is-conflict-alert" : ""} disabled={saving}
                        onClick={() => { void toggleConflicts(binding.id); }}>
                        ⚠ Xung đột ({openCount})
                      </button>
                      {(binding.state === "draft" || binding.state === "active") && (
                        <button type="button" disabled={saving}
                          onClick={() => { void changeState(binding, "pause"); }}>Tạm dừng</button>
                      )}
                      {binding.state !== "revoked" && (
                        <button type="button" disabled={saving}
                          onClick={() => { void changeState(binding, "revoke"); }}>Thu hồi</button>
                      )}
                    </div>

                    {pairingOpenId === binding.id && (
                      <div className="integration-pairing-panel">
                        <h6>🔑 Ghép nối trực tiếp QGIS Desktop (Pha A)</h6>
                        <div className="integration-pairing-row">
                          <label>
                            Tên máy / thiết bị
                            <input
                              type="text"
                              value={pairingLabel}
                              maxLength={120}
                              disabled={saving}
                              onChange={(e) => setPairingLabel(e.target.value)}
                            />
                          </label>
                          <label>
                            Thời hạn
                            <select value={pairingDays} disabled={saving} onChange={(e) => setPairingDays(e.target.value)}>
                              <option value="7">7 ngày</option>
                              <option value="30">30 ngày</option>
                              <option value="90">90 ngày</option>
                            </select>
                          </label>
                          <button type="button" className="is-pair" disabled={saving}
                            onClick={() => { void issuePairing(binding); }}>
                            {saving ? "Đang tạo…" : "Tạo mã ghép nối mới"}
                          </button>
                        </div>

                        {issuedPairing && issuedPairing.binding.id === binding.id && (
                          <div className="integration-pairing-secret" role="status">
                            <strong>Sao chép Mã ghép nối nhanh (chỉ hiển thị 1 lần — dán thẳng vào QGIS Plugin 0.4.0):</strong>
                            <input readOnly aria-label="Mã ghép nối nhanh QGIS" value={issuedPairing.pairing_code} />
                            <div className="integration-binding-actions">
                              <button type="button" className="is-pair" onClick={() => { void copyText(issuedPairing.pairing_code, "Mã ghép nối QGIS"); }}>
                                📋 Sao chép Mã ghép nối
                              </button>
                              <button type="button" onClick={() => { void copyText(issuedPairing.bearer_token, "Khóa Bearer"); }}>
                                Sao chép khóa Bearer lẻ
                              </button>
                              <button type="button" onClick={() => setIssuedPairing(null)}>Ẩn mã</button>
                            </div>
                          </div>
                        )}

                        {tokens.length > 0 && (
                          <div className="integration-token-list">
                            {tokens.map((t) => (
                              <div key={t.id} className="integration-token-row">
                                <span>
                                  <strong>{t.label}</strong> · <code>{t.token_prefix}…</code> · Hết hạn: {new Date(t.expires_at).toLocaleDateString("vi-VN")}
                                  {t.revoked_at ? " · Đã thu hồi" : " · Đang hiệu lực"}
                                </span>
                                {!t.revoked_at && (
                                  <button type="button" disabled={saving} onClick={() => { void revokeToken(binding.id, t.id); }}>
                                    Thu hồi mã
                                  </button>
                                )}
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    )}

                    {conflictsOpenId === binding.id && (
                      <div className="integration-conflict-panel">
                        <h6>⚠ Giải quyết xung đột Web ↔ QGIS (Pha B)</h6>
                        {conflicts.length === 0 ? (
                          <p>Không có xung đột nào được ghi nhận trên cấu hình này.</p>
                        ) : (
                          <div className="integration-conflict-list">
                            {conflicts.map((c) => (
                              <div key={c.id} className={`integration-conflict-card is-${c.status}`}>
                                <div className="integration-conflict-head">
                                  <strong>Đối tượng: <code>{c.feature_id}</code>{c.field_name ? ` · Trường: ${c.field_name}` : ""} ({c.conflict_kind})</strong>
                                  <span className={`conflict-pill is-${c.status}`}>{conflictStatusLabel(c.status)}</span>
                                </div>
                                <div className="integration-conflict-diff">
                                  <div>
                                    <small>Bản gốc (Baseline)</small>
                                    <pre>{formatJsonValue(c.baseline_value)}</pre>
                                  </div>
                                  <div className="is-web">
                                    <small>Bản Web (OpenVietGIS)</small>
                                    <pre>{formatJsonValue(c.web_value)}</pre>
                                  </div>
                                  <div className="is-qgis">
                                    <small>Bản QGIS Desktop</small>
                                    <pre>{formatJsonValue(c.qgis_value)}</pre>
                                  </div>
                                </div>
                                {c.status === "open" ? (
                                  <div className="integration-conflict-actions">
                                    <button type="button" disabled={saving} onClick={() => { void resolveConflict(binding.id, c, "keep_web"); }}>
                                      Giữ bản Web
                                    </button>
                                    <button type="button" disabled={saving} onClick={() => { void resolveConflict(binding.id, c, "keep_qgis"); }}>
                                      Giữ bản QGIS
                                    </button>
                                    <input
                                      type="text"
                                      placeholder="Hoặc nhập giá trị gộp tùy chỉnh…"
                                      value={mergeDrafts[c.id] ?? ""}
                                      disabled={saving}
                                      onChange={(e) => setMergeDrafts((prev) => ({ ...prev, [c.id]: e.target.value }))}
                                    />
                                    <button type="button" disabled={saving} onClick={() => { void resolveConflict(binding.id, c, "merge"); }}>
                                      Lưu giá trị gộp
                                    </button>
                                    <button type="button" disabled={saving} onClick={() => { void resolveConflict(binding.id, c, "dismiss"); }}>
                                      Bỏ qua
                                    </button>
                                  </div>
                                ) : (
                                  <small>Giá trị đã chốt: <code>{formatJsonValue(c.resolved_value)}</code></small>
                                )}
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                  </article>
                );
              })}
            </div>
          )}
        </>
      )}
    </section>
  );
}
