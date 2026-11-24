import { useEffect, useState } from "react";
import {
  ApiError,
  activateQgisBinding,
  changeQgisBindingDraftLifecycle,
  createQgisBindingDraft,
  issueBindingPairingToken,
  listBindingConflicts,
  listBindingPairingTokens,
  listChangesets,
  listIntegrationBindingDrafts,
  listQgisBindingCandidates,
  loadCsrfToken,
  resolveBindingConflict,
  revertChangeset,
  revokeBindingPairingToken,
  type ApiClient,
  type BindingPairingToken,
  type Changeset,
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

function slugPart(raw: string): string {
  const cleaned = raw
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^A-Za-z0-9._-]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 48);
  return cleaned || "layer";
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

  // Compact multi-layer selection
  const [selectedLayers, setSelectedLayers] = useState<string[]>([]);
  const [datasetPrefix, setDatasetPrefix] = useState("qgis_sync");

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

  // Version History & Revert state (Google Sheets style via R006)
  const [historyOpenId, setHistoryOpenId] = useState<string | null>(null);
  const [historyByBinding, setHistoryByBinding] = useState<Record<string, Changeset[]>>({});

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError("");
    setForbidden(false);
    setNotice("");
    setSelectedLayers([]);
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

  async function copyText(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      setNotice(`Đã sao chép ${label}.`);
    } catch {
      setError(`Không thể tự sao chép ${label}. Hãy bôi đen và sao chép thủ công.`);
    }
  }

  function toggleLayer(id: string) {
    setSelectedLayers((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id]
    );
    setError("");
  }

  async function createBatchBindings() {
    const targets = options.filter((item) => selectedLayers.includes(item.layer_id));
    if (targets.length === 0 || saving) return;
    const prefix = slugPart(datasetPrefix.trim() || "qgis_sync");
    if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/.test(prefix)) {
      setError("Tiền tố mã chỉ được dùng chữ không dấu, số, dấu chấm, gạch nối hoặc gạch dưới.");
      return;
    }
    setSaving(true);
    setError("");
    setNotice("");
    let createdCount = 0;
    let skippedCount = 0;
    const added: IntegrationBindingDraft[] = [];
    try {
      const csrf = await loadCsrfToken(client);
      for (let i = 0; i < targets.length; i += 1) {
        const target = targets[i]!;
        const refName = targets.length === 1
          ? prefix
          : `${prefix}_${slugPart(target.layer_name)}`.slice(0, 128);
        try {
          const draft = await createQgisBindingDraft(
            client,
            org,
            projectId,
            {
              adapter_key: "qgis",
              layer_id: target.layer_id,
              external_dataset_ref: refName,
              direction: "bidirectional_manual",
              source_crs_code: target.stored_crs_code,
              target_crs_code: target.stored_crs_code,
              external_primary_key: "ovg_feature_id",
              allowed_field_ids: target.allowed_field_ids.slice(0, 100)
            },
            csrf
          );
          const activated = await activateQgisBinding(
            client,
            org,
            projectId,
            draft.id,
            draft.revision,
            csrf
          );
          added.push(activated);
          createdCount += 1;
        } catch (reason: unknown) {
          if (reason instanceof ApiError && reason.status === 409) {
            skippedCount += 1;
          } else {
            throw reason;
          }
        }
      }
      if (added.length > 0) {
        setBindings((prev) => [...added, ...prev]);
      }
      setSelectedLayers([]);
      setNotice(
        `Đã tạo & kích hoạt ${createdCount} cấu hình QGIS` +
        (skippedCount > 0 ? ` (bỏ qua ${skippedCount} cấu hình đã tồn tại).` : ".")
      );
    } catch {
      setError("Không thể tạo cấu hình cho một số lớp đã chọn.");
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
      setNotice(`Đã kích hoạt '${binding.external_dataset_ref}'.`);
    } catch {
      setError("Không thể kích hoạt cấu hình.");
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
      setNotice(action === "pause" ? "Đã tạm dừng." : "Đã thu hồi cấu hình & mã ghép nối.");
    } catch {
      setError("Không thể cập nhật trạng thái cấu hình.");
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
      setNotice("Đã tạo mã ghép nối QGIS — sao chép ngay (chỉ hiện 1 lần).");
    } catch {
      setError("Không thể tạo mã ghép nối QGIS.");
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
      setNotice("Đã thu hồi mã ghép nối.");
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
        setError("Nhập giá trị gộp trước khi lưu.");
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
      setNotice(`Đã xử lý xung đột ${conflict.feature_id}: ${conflictStatusLabel(updated.status)}.`);
    } catch {
      setError("Không thể lưu quyết định xử lý xung đột.");
    } finally {
      setSaving(false);
    }
  }

  async function toggleHistory(binding: IntegrationBindingDraft) {
    if (historyOpenId === binding.id) {
      setHistoryOpenId(null);
      return;
    }
    const sourceId = options.find((item) => item.layer_id === binding.layer_id)?.source_id;
    if (!sourceId) {
      setError("Không tìm thấy nguồn dữ liệu (source_id) của lớp này.");
      return;
    }
    setHistoryOpenId(binding.id);
    setError("");
    try {
      const res = await listChangesets(client, org, projectId, sourceId, 30);
      setHistoryByBinding((prev) => ({ ...prev, [binding.id]: res.items }));
    } catch {
      setError("Không thể tải lịch sử phiên bản R006.");
    }
  }

  async function undoVersion(binding: IntegrationBindingDraft, cs: Changeset) {
    if (saving) return;
    if (!confirm(`Hoàn tác phiên bản "${cs.summary}" (${cs.operation_count} thao tác)? Mọi đối tượng bị sửa hoặc xóa trong phiên bản này sẽ được khôi phục nguyên vẹn.`)) {
      return;
    }
    setSaving(true);
    setError("");
    setNotice("");
    try {
      const csrf = await loadCsrfToken(client);
      const opId = typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `revert-${Date.now()}`;
      await revertChangeset(
        client,
        org,
        projectId,
        cs.id,
        opId,
        csrf,
        `Hoàn tác phiên bản: ${cs.summary}`.slice(0, 200)
      );
      const sourceId = options.find((item) => item.layer_id === binding.layer_id)?.source_id;
      if (sourceId) {
        const refreshed = await listChangesets(client, org, projectId, sourceId, 30);
        setHistoryByBinding((prev) => ({ ...prev, [binding.id]: refreshed.items }));
      }
      setNotice(`✓ Đã khôi phục phiên bản thành công! QGIS và Web đang tự cập nhật realtime.`);
    } catch {
      setError("Không thể hoàn tác phiên bản (có thể dữ liệu đã thay đổi hoặc đã được hoàn tác).");
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="integration-binding-manager is-compact" aria-label="Quản lý cấu hình QGIS R027">
      <header className="integration-binding-header">
        <h4>QGIS – Đồng bộ 2 chiều & Mã ghép nối (R027)</h4>
        <div className="integration-binding-links">
          <a
            href="/rewrite/downloads/openvietgis_connector-0.8.0.zip"
            download="openvietgis_connector-0.8.0.zip"
          >⬇ Plugin QGIS 0.8.0 (Realtime · Cả Workspace · Hoàn tác)</a>
          {" · "}
          <a
            href="https://github.com/nguyentrieu210/OpenVietGis/blob/main/integrations/qgis_plugin/openvietgis_connector/R027-MANUAL-SYNC.md"
            target="_blank"
            rel="noopener noreferrer"
          >Hướng dẫn</a>
        </div>
      </header>
      <p className="integration-binding-sub">
        Chọn 1 hoặc nhiều lớp bên dưới để tạo & kích hoạt kết nối QGIS 2 chiều. Không tự ghi/xóa ngầm.
      </p>

      {loading && <p role="status">Đang tải danh sách lớp…</p>}
      {forbidden && <p role="status">Cần quyền quản trị dự án và phạm vi GIS đầy đủ.</p>}
      {error && <p className="error" role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}

      {!loading && !forbidden && (
        <>
          <div className="integration-compact-picker">
            <div className="integration-compact-picker-bar">
              <strong>Chọn lớp ({selectedLayers.length}/{options.length})</strong>
              <div className="integration-binding-actions">
                <button
                  type="button"
                  disabled={saving || options.length === 0}
                  onClick={() => setSelectedLayers(options.map((o) => o.layer_id))}
                >☑ Chọn tất cả</button>
                <button
                  type="button"
                  disabled={saving || selectedLayers.length === 0}
                  onClick={() => setSelectedLayers([])}
                >☐ Bỏ chọn</button>
              </div>
            </div>
            <div className="integration-layer-checklist">
              {options.map((item) => (
                <label key={item.layer_id} className={selectedLayers.includes(item.layer_id) ? "is-checked" : ""}>
                  <input
                    type="checkbox"
                    checked={selectedLayers.includes(item.layer_id)}
                    disabled={saving}
                    onChange={() => toggleLayer(item.layer_id)}
                  />
                  <span>
                    <b>{item.layer_name}</b>
                    <small>EPSG:{item.stored_crs_code} · {item.allowed_field_ids.length} trường</small>
                  </span>
                </label>
              ))}
            </div>
            <div className="integration-compact-create-row">
              <input
                type="text"
                value={datasetPrefix}
                maxLength={64}
                autoComplete="off"
                placeholder="Tiền tố mã (VD: qgis_sync)"
                disabled={saving}
                onChange={(e) => setDatasetPrefix(e.target.value)}
              />
              <button
                type="button"
                className="integration-binding-create"
                disabled={saving || selectedLayers.length === 0}
                onClick={() => { void createBatchBindings(); }}
              >
                {saving
                  ? "Đang xử lý…"
                  : `＋ Tạo & Kích hoạt (${selectedLayers.length} lớp)`}
              </button>
            </div>
          </div>

          <h5 className="integration-compact-heading">Cấu hình đang có ({bindings.length})</h5>
          {bindings.length === 0 ? (
            <p className="integration-binding-sub">Chưa có cấu hình. Tích chọn các lớp ở trên rồi bấm Tạo & Kích hoạt.</p>
          ) : (
            <div className="integration-binding-list is-compact">
              {bindings.map((binding) => {
                const openCount = binding.open_conflicts_count ?? 0;
                const tokens = pairingTokens[binding.id] ?? [];
                const conflicts = conflictsByBinding[binding.id] ?? [];
                return (
                  <article key={binding.id} className={binding.state === "active" ? "is-active-binding" : ""}>
                    <div className="integration-binding-summary">
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
                          onClick={() => { void togglePairing(binding.id); }}>🔑 Mã ghép nối</button>
                      )}
                      <button type="button" className={openCount > 0 ? "is-conflict-alert" : ""} disabled={saving}
                        onClick={() => { void toggleConflicts(binding.id); }}>
                        ⚠ Xung đột ({openCount})
                      </button>
                      <button type="button" disabled={saving}
                        onClick={() => { void toggleHistory(binding); }}>
                        🕒 Lịch sử
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
                        <div className="integration-pairing-row">
                          <label>
                            Tên máy
                            <input
                              type="text"
                              value={pairingLabel}
                              maxLength={120}
                              disabled={saving}
                              onChange={(e) => setPairingLabel(e.target.value)}
                            />
                          </label>
                          <label>
                            Hạn
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
                            <strong>Mã ghép nối nhanh (chỉ hiện 1 lần — dán vào QGIS Plugin 0.7.0):</strong>
                            <input readOnly aria-label="Mã ghép nối nhanh QGIS" value={issuedPairing.pairing_code} />
                            <div className="integration-binding-actions">
                              <button type="button" className="is-pair" onClick={() => { void copyText(issuedPairing.pairing_code, "Mã ghép nối QGIS"); }}>
                                📋 Sao chép Mã ghép nối
                              </button>
                              <button type="button" onClick={() => { void copyText(issuedPairing.bearer_token, "Khóa Bearer"); }}>
                                Copy Bearer lẻ
                              </button>
                              <button type="button" onClick={() => setIssuedPairing(null)}>Ẩn</button>
                            </div>
                          </div>
                        )}

                        {tokens.length > 0 && (
                          <div className="integration-token-list">
                            {tokens.map((t) => (
                              <div key={t.id} className="integration-token-row">
                                <span>
                                  <strong>{t.label}</strong> · <code>{t.token_prefix}…</code> · Hết hạn {new Date(t.expires_at).toLocaleDateString("vi-VN")}
                                  {t.revoked_at ? " · Đã thu hồi" : " · Đang hiệu lực"}
                                </span>
                                {!t.revoked_at && (
                                  <button type="button" disabled={saving} onClick={() => { void revokeToken(binding.id, t.id); }}>
                                    Thu hồi
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
                        <h6>⚠ Giải quyết xung đột Web ↔ QGIS</h6>
                        {conflicts.length === 0 ? (
                          <p>Không có xung đột nào trên cấu hình này.</p>
                        ) : (
                          <div className="integration-conflict-list">
                            {conflicts.map((c) => (
                              <div key={c.id} className={`integration-conflict-card is-${c.status}`}>
                                <div className="integration-conflict-head">
                                  <strong>Đối tượng: <code>{c.feature_id}</code>{c.field_name ? ` · ${c.field_name}` : ""} ({c.conflict_kind})</strong>
                                  <span className={`conflict-pill is-${c.status}`}>{conflictStatusLabel(c.status)}</span>
                                </div>
                                <div className="integration-conflict-diff">
                                  <div><small>Bản gốc</small><pre>{formatJsonValue(c.baseline_value)}</pre></div>
                                  <div className="is-web"><small>Bản Web</small><pre>{formatJsonValue(c.web_value)}</pre></div>
                                  <div className="is-qgis"><small>Bản QGIS</small><pre>{formatJsonValue(c.qgis_value)}</pre></div>
                                </div>
                                {c.status === "open" ? (
                                  <div className="integration-conflict-actions">
                                    <button type="button" disabled={saving} onClick={() => { void resolveConflict(binding.id, c, "keep_web"); }}>Giữ bản Web</button>
                                    <button type="button" disabled={saving} onClick={() => { void resolveConflict(binding.id, c, "keep_qgis"); }}>Giữ bản QGIS</button>
                                    <input
                                      type="text"
                                      placeholder="Giá trị gộp tùy chỉnh…"
                                      value={mergeDrafts[c.id] ?? ""}
                                      disabled={saving}
                                      onChange={(e) => setMergeDrafts((prev) => ({ ...prev, [c.id]: e.target.value }))}
                                    />
                                    <button type="button" disabled={saving} onClick={() => { void resolveConflict(binding.id, c, "merge"); }}>Lưu gộp</button>
                                    <button type="button" disabled={saving} onClick={() => { void resolveConflict(binding.id, c, "dismiss"); }}>Bỏ qua</button>
                                  </div>
                                ) : (
                                  <small>Đã chốt: <code>{formatJsonValue(c.resolved_value)}</code></small>
                                )}
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    )}

                    {historyOpenId === binding.id && (
                      <div className="integration-conflict-panel">
                        <h6>🕒 Lịch sử phiên bản & Hoàn tác (giống Google Sheets · R006)</h6>
                        {(historyByBinding[binding.id] ?? []).length === 0 ? (
                          <p>Chưa có phiên bản chỉnh sửa nào trên lớp này.</p>
                        ) : (
                          <div className="integration-token-list">
                            {(historyByBinding[binding.id] ?? []).map((cs) => (
                              <div key={cs.id} className="integration-token-row">
                                <span>
                                  <strong>{cs.summary}</strong> · {cs.actor_display_snapshot} ·{" "}
                                  {cs.operation_count} thao tác ·{" "}
                                  {new Date(cs.created_at).toLocaleString("vi-VN")}
                                  {cs.reverted_from_changeset_id ? " · (Bản khôi phục)" : ""}
                                </span>
                                <button
                                  type="button"
                                  disabled={saving}
                                  onClick={() => { void undoVersion(binding, cs); }}
                                >
                                  ↩ Hoàn tác
                                </button>
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
