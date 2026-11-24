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
  listManagedSessions,
  listQgisBindingCandidates,
  loadCsrfToken,
  resolveBindingConflict,
  revertChangeset,
  revokeBindingPairingToken,
  revokeManagedSession,
  sendDesktopCommand,
  type ApiClient,
  type BindingPairingToken,
  type Changeset,
  type IntegrationBindingDraft,
  type IssuedPairingTokenResponse,
  type ManagedSessionItem,
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

function geomLabel(family: string): string {
  const f = family.toLowerCase();
  if (f.includes("point")) return "Điểm";
  if (f.includes("line")) return "Đường";
  if (f.includes("poly")) return "Vùng";
  return family;
}

export function QgisBindingManager({ client, org, projectId }: Props) {
  const [options, setOptions] = useState<QgisBindingCandidate[]>([]);
  const [bindings, setBindings] = useState<IntegrationBindingDraft[]>([]);
  const [sessions, setSessions] = useState<ManagedSessionItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [forbidden, setForbidden] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  // Workspace pairing code drawer
  const [pairingOpen, setPairingOpen] = useState(false);
  const [issuedBundle, setIssuedBundle] = useState<IssuedPairingTokenResponse | null>(null);
  const [tokens, setTokens] = useState<BindingPairingToken[]>([]);
  const [devicesOpen, setDevicesOpen] = useState(false);

  // Per-layer drawers (History / Conflicts)
  const [historyLayerId, setHistoryLayerId] = useState<string | null>(null);
  const [historyItems, setHistoryItems] = useState<Changeset[]>([]);
  const [conflictBindingId, setConflictBindingId] = useState<string | null>(null);
  const [conflicts, setConflicts] = useState<SyncConflictRecord[]>([]);
  const [mergeDrafts, setMergeDrafts] = useState<Record<string, string>>({});

  async function refreshAll() {
    setLoading(true);
    setForbidden(false);
    setError("");
    try {
      const [cands, drafts, sessRes] = await Promise.all([
        listQgisBindingCandidates(client, org, projectId),
        listIntegrationBindingDrafts(client, org, projectId),
        listManagedSessions(client, org, projectId).catch(() => ({
          current_cursor: 0,
          permission_epoch: 1,
          retention_floor: 0,
          items: [] as ManagedSessionItem[]
        }))
      ]);
      setOptions(cands.items);
      setBindings(drafts.items);
      setSessions(sessRes.items);
    } catch (err) {
      if (err instanceof ApiError && err.status === 403) {
        setForbidden(true);
      } else {
        setError("Không thể tải cấu hình kết nối QGIS.");
      }
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refreshAll();
  }, [client, org, projectId]);

  const activeSessions = sessions.filter((s) => s.state === "active");

  function findBindingForLayer(layerId: string): IntegrationBindingDraft | undefined {
    return (
      bindings.find((b) => b.layer_id === layerId && b.state === "active") ??
      bindings.find((b) => b.layer_id === layerId && b.state !== "revoked")
    );
  }

  async function ensureBindingActive(layer: QgisBindingCandidate): Promise<IntegrationBindingDraft> {
    const csrf = await loadCsrfToken(client);
    const existing = findBindingForLayer(layer.layer_id);
    if (existing && existing.state === "active") {
      return existing;
    }
    if (existing && (existing.state === "draft" || existing.state === "paused")) {
      return activateQgisBinding(client, org, projectId, existing.id, existing.revision, csrf);
    }
    const created = await createQgisBindingDraft(
      client,
      org,
      projectId,
      {
        layer_id: layer.layer_id,
        adapter_key: "qgis",
        external_dataset_ref: `${slugPart(layer.layer_name)}_${layer.layer_id.slice(0, 6)}`,
        direction: "bidirectional_manual",
        source_crs_code: layer.stored_crs_code,
        target_crs_code: layer.stored_crs_code,
        external_primary_key: "ovg_feature_id",
        allowed_field_ids: layer.allowed_field_ids
      },
      csrf
    );
    return activateQgisBinding(client, org, projectId, created.id, created.revision, csrf);
  }

  async function handleToggleLayer(layer: QgisBindingCandidate) {
    if (saving) return;
    setSaving(true);
    setError("");
    setNotice("");
    try {
      const existing = findBindingForLayer(layer.layer_id);
      if (existing && existing.state === "active") {
        const csrf = await loadCsrfToken(client);
        await changeQgisBindingDraftLifecycle(
          client,
          org,
          projectId,
          existing.id,
          "pause",
          existing.revision,
          csrf
        );
        setNotice(`Đã tạm tắt đồng bộ lớp "${layer.layer_name}".`);
      } else {
        await ensureBindingActive(layer);
        setNotice(`✓ Đã bật đồng bộ 2 chiều cho lớp "${layer.layer_name}".`);
      }
      await refreshAll();
    } catch {
      setError(`Không thể cập nhật trạng thái đồng bộ cho lớp "${layer.layer_name}".`);
    } finally {
      setSaving(false);
    }
  }

  async function handleQuickPairingCode() {
    if (saving) return;
    if (options.length === 0) {
      setError("Dự án chưa có lớp bản đồ hợp lệ để kết nối.");
      return;
    }
    setSaving(true);
    setError("");
    setNotice("");
    try {
      let targetBinding = bindings.find((b) => b.state === "active");
      if (!targetBinding) {
        targetBinding = await ensureBindingActive(options[0]);
      }
      const csrf = await loadCsrfToken(client);
      const issued = await issueBindingPairingToken(
        client,
        org,
        projectId,
        targetBinding.id,
        { label: "QGIS Desktop", expires_in_days: 30 },
        csrf
      );
      setIssuedBundle(issued);
      setPairingOpen(true);
      const tokRes = await listBindingPairingTokens(client, org, projectId, targetBinding.id);
      setTokens(tokRes.items);
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(issued.pairing_code).catch(() => {});
        setNotice("✓ Đã tạo & tự động sao chép Mã kết nối! Dán thẳng vào Plugin QGIS.");
      } else {
        setNotice("✓ Đã tạo Mã kết nối! Sao chép mã bên dưới và dán vào Plugin QGIS.");
      }
      await refreshAll();
    } catch {
      setError("Không thể tạo mã kết nối QGIS. Kiểm tra quyền quản trị dự án.");
    } finally {
      setSaving(false);
    }
  }

  async function handleRevokeToken(bindingId: string, tokenId: string) {
    if (saving) return;
    setSaving(true);
    setError("");
    try {
      const csrf = await loadCsrfToken(client);
      await revokeBindingPairingToken(client, org, projectId, bindingId, tokenId, csrf);
      const tokRes = await listBindingPairingTokens(client, org, projectId, bindingId);
      setTokens(tokRes.items);
      setNotice("Đã thu hồi mã kết nối.");
    } catch {
      setError("Không thể thu hồi mã kết nối.");
    } finally {
      setSaving(false);
    }
  }

  async function handleRevokeSession(sessionId: string) {
    if (saving) return;
    setSaving(true);
    setError("");
    try {
      const csrf = await loadCsrfToken(client);
      await revokeManagedSession(client, org, projectId, sessionId, csrf, true);
      await refreshAll();
      setNotice("🔒 Đã ngắt kết nối máy QGIS — máy trạm đã chuyển về Chỉ đọc.");
    } catch {
      setError("Không thể ngắt kết nối phiên QGIS.");
    } finally {
      setSaving(false);
    }
  }

  async function handlePingDesktop(sessionId: string) {
    if (saving) return;
    setSaving(true);
    setError("");
    try {
      const csrf = await loadCsrfToken(client);
      await sendDesktopCommand(
        client,
        org,
        projectId,
        sessionId,
        { tool_name: "inspect_project", arguments: {} },
        csrf
      );
      setNotice("🤖 Đã gửi yêu cầu kiểm tra sang máy QGIS.");
    } catch {
      setError("Không thể gửi lệnh sang QGIS.");
    } finally {
      setSaving(false);
    }
  }

  async function toggleHistory(layer: QgisBindingCandidate) {
    if (historyLayerId === layer.layer_id) {
      setHistoryLayerId(null);
      return;
    }
    if (!layer.source_id) {
      setError("Không tìm thấy lịch sử phiên bản cho lớp này.");
      return;
    }
    setConflictBindingId(null);
    setHistoryLayerId(layer.layer_id);
    setError("");
    try {
      const res = await listChangesets(client, org, projectId, layer.source_id);
      setHistoryItems(res.items);
    } catch {
      setError("Không thể tải lịch sử phiên bản.");
    }
  }

  async function handleUndoVersion(layer: QgisBindingCandidate, cs: Changeset) {
    if (saving || !layer.source_id) return;
    if (!confirm(`Hoàn tác "${cs.summary}" (${cs.operation_count} thay đổi)?`)) return;
    setSaving(true);
    setError("");
    try {
      const csrf = await loadCsrfToken(client);
      const opId =
        typeof crypto !== "undefined" && "randomUUID" in crypto
          ? crypto.randomUUID()
          : `revert-${Date.now()}`;
      await revertChangeset(
        client,
        org,
        projectId,
        cs.id,
        opId,
        csrf,
        `Hoàn tác: ${cs.summary}`.slice(0, 200)
      );
      const refreshed = await listChangesets(client, org, projectId, layer.source_id);
      setHistoryItems(refreshed.items);
      setNotice("✓ Đã khôi phục phiên bản thành công!");
    } catch {
      setError("Không thể hoàn tác phiên bản này.");
    } finally {
      setSaving(false);
    }
  }

  async function toggleConflicts(bindingId: string) {
    if (conflictBindingId === bindingId) {
      setConflictBindingId(null);
      return;
    }
    setHistoryLayerId(null);
    setConflictBindingId(bindingId);
    setError("");
    try {
      const res = await listBindingConflicts(client, org, projectId, bindingId);
      setConflicts(res.items);
    } catch {
      setError("Không thể tải danh sách xung đột.");
    }
  }

  async function handleResolveConflict(
    bindingId: string,
    conflict: SyncConflictRecord,
    resolution: "keep_web" | "keep_qgis" | "merge" | "dismiss"
  ) {
    if (saving) return;
    setSaving(true);
    setError("");
    try {
      const csrf = await loadCsrfToken(client);
      const payload: { decision: "keep_web" | "keep_qgis" | "merge" | "dismiss"; merged_value?: unknown } = {
        decision: resolution
      };
      if (resolution === "merge") {
        payload.merged_value = mergeDrafts[conflict.id] ?? "";
      }
      await resolveBindingConflict(client, org, projectId, bindingId, conflict.id, payload, csrf);
      const refreshed = await listBindingConflicts(client, org, projectId, bindingId);
      setConflicts(refreshed.items);
      await refreshAll();
      setNotice("✓ Đã xử lý xung đột.");
    } catch {
      setError("Không thể lưu quyết định xử lý xung đột.");
    } finally {
      setSaving(false);
    }
  }

  const activeBindingCount = options.filter((o) => findBindingForLayer(o.layer_id)?.state === "active").length;

  return (
    <section className="qgis-clean-shell" aria-label="Đồng bộ 2 chiều QGIS">
      {/* Block 1: Single Workspace Connection Card */}
      <div className="qgis-hero-card">
        <div className="qgis-hero-left">
          <div className="qgis-hero-status-row">
            <h4>Đồng bộ 2 chiều QGIS Desktop</h4>
            <span className={`qgis-status-pill ${activeSessions.length > 0 ? "is-online" : ""}`}>
              {activeSessions.length > 0
                ? `● ${activeSessions.length} máy đang kết nối`
                : "○ Chưa kết nối"}
            </span>
          </div>
          <p>
            Lấy mã kết nối, mở bản đồ trong QGIS và bấm Lưu để gửi thay đổi lên Web. Khi có cập nhật từ nhóm, công cụ sẽ thông báo và tải phần thay đổi mới.
          </p>
        </div>
        <div className="qgis-hero-actions">
          <button
            type="button"
            className="qgis-btn-primary"
            disabled={saving || loading || forbidden}
            onClick={() => { void handleQuickPairingCode(); }}
          >
            🔑 Lấy mã kết nối
          </button>
          <a
            className="qgis-btn-secondary"
            href="/rewrite/downloads/openvietgis_connector-1.0.0-f17a7e9.zip"
            download="openvietgis_connector-1.0.0.zip"
          >
            ⬇ Tải công cụ QGIS
          </a>
        </div>
      </div>

      {/* Inline status / error toast */}
      {error && <p className="qgis-alert is-error" role="alert">{error}</p>}
      {notice && <p className="qgis-alert is-ok" role="status">{notice}</p>}
      {forbidden && (
        <p className="qgis-alert is-muted" role="status">
          Cần quyền quản trị dự án để bật đồng bộ hoặc cấp mã kết nối QGIS.
        </p>
      )}

      {/* Issued Pairing Code Panel (only shown when user clicks "Lấy mã kết nối") */}
      {pairingOpen && issuedBundle && (
        <div className="qgis-drawer">
          <div className="qgis-drawer-head">
            <strong>Mã kết nối QGIS (dùng cho toàn bộ dự án)</strong>
            <button type="button" className="qgis-btn-ghost" onClick={() => setPairingOpen(false)}>Đóng</button>
          </div>
          <div className="qgis-code-box">
            <code>{issuedBundle.pairing_code}</code>
            <button
              type="button"
              className="qgis-btn-primary is-sm"
              onClick={() => {
                void navigator.clipboard?.writeText(issuedBundle.pairing_code);
                setNotice("✓ Đã sao chép mã kết nối.");
              }}
            >
              Sao chép
            </button>
          </div>
          {tokens.length > 0 && (
            <div className="qgis-sub-list">
              {tokens.slice(0, 4).map((t) => (
                <div key={t.id} className="qgis-sub-row">
                  <span>{t.label} · <code>{t.token_prefix}…</code> · Hết hạn {new Date(t.expires_at).toLocaleDateString("vi-VN")}</span>
                  {!t.revoked_at && (
                    <button
                      type="button"
                      className="qgis-btn-ghost is-danger"
                      disabled={saving}
                      onClick={() => { void handleRevokeToken(t.binding_id, t.id); }}
                    >
                      Thu hồi
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Connected Devices Bar (compact expandable) */}
      {!loading && !forbidden && (
        <div className="qgis-section-bar">
          <span className="qgis-section-title">
            Lớp bản đồ đồng bộ ({activeBindingCount}/{options.length})
          </span>
          <div className="qgis-section-tools">
            <button
              type="button"
              className="qgis-btn-ghost"
              onClick={() => setDevicesOpen((v) => !v)}
            >
              💻 Thiết bị đang kết nối ({activeSessions.length}) {devicesOpen ? "▴" : "▾"}
            </button>
            <button
              type="button"
              className="qgis-btn-ghost"
              disabled={loading}
              onClick={() => { void refreshAll(); }}
            >
              ↻ Làm mới
            </button>
          </div>
        </div>
      )}

      {devicesOpen && (
        <div className="qgis-drawer">
          <div className="qgis-drawer-head">
            <strong>Máy trạm QGIS đang kết nối</strong>
          </div>
          {sessions.length === 0 ? (
            <p className="qgis-empty">Chưa có máy QGIS nào kết nối trực tiếp vào dự án này.</p>
          ) : (
            <div className="qgis-sub-list">
              {sessions.slice(0, 8).map((s) => (
                <div key={s.id} className="qgis-sub-row">
                  <span>
                    <strong>{s.device_label}</strong> · {s.actor_display} ·{" "}
                    {s.capabilities.can_edit ? "Sửa 2 chiều" : "Chỉ đọc"} ·{" "}
                    {s.state === "active" ? "🟢 Đang hoạt động" : "⚪ Đã ngắt"}
                  </span>
                  {s.state === "active" && (
                    <span className="qgis-row-actions">
                      <button
                        type="button"
                        className="qgis-btn-ghost"
                        disabled={saving}
                        onClick={() => { void handlePingDesktop(s.id); }}
                      >
                        Kiểm tra
                      </button>
                      <button
                        type="button"
                        className="qgis-btn-ghost is-danger"
                        disabled={saving}
                        onClick={() => { void handleRevokeSession(s.id); }}
                      >
                        Ngắt kết nối
                      </button>
                    </span>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Block 2: Clean Layer Toggle List */}
      {loading && <p className="qgis-empty" role="status">Đang tải danh sách lớp…</p>}

      {!loading && !forbidden && (
        <div className="qgis-layer-table">
          {options.length === 0 ? (
            <p className="qgis-empty">Dự án chưa có lớp bản đồ nào đủ điều kiện đồng bộ.</p>
          ) : (
            options.map((layer) => {
              const binding = findBindingForLayer(layer.layer_id);
              const isOn = binding?.state === "active";
              const openConflicts = binding?.open_conflicts_count ?? 0;

              return (
                <div key={layer.layer_id} className="qgis-layer-item">
                  <div className="qgis-layer-row">
                    <div className="qgis-layer-info">
                      <strong>{layer.layer_name}</strong>
                      <small>{geomLabel(layer.geometry_family)} · EPSG:{layer.stored_crs_code}</small>
                    </div>

                    <div className="qgis-row-actions">
                      {binding && openConflicts > 0 && (
                        <button
                          type="button"
                          className="qgis-conflict-chip"
                          onClick={() => { void toggleConflicts(binding.id); }}
                        >
                          ⚠ {openConflicts} xung đột
                        </button>
                      )}
                      <button
                        type="button"
                        className="qgis-btn-ghost"
                        onClick={() => { void toggleHistory(layer); }}
                      >
                        🕒 Lịch sử
                      </button>
                      <button
                        type="button"
                        role="switch"
                        aria-checked={isOn}
                        aria-label={`Bật đồng bộ 2 chiều cho ${layer.layer_name}`}
                        disabled={saving}
                        className={`qgis-switch ${isOn ? "is-on" : ""}`}
                        onClick={() => { void handleToggleLayer(layer); }}
                      >
                        <span className="qgis-switch-knob" />
                      </button>
                    </div>
                  </div>

                  {historyLayerId === layer.layer_id && (
                    <div className="qgis-inline-drawer">
                      <div className="qgis-drawer-head">
                        <strong>Lịch sử phiên bản & Hoàn tác</strong>
                        <button type="button" className="qgis-btn-ghost" onClick={() => setHistoryLayerId(null)}>Đóng</button>
                      </div>
                      {historyItems.length === 0 ? (
                        <p className="qgis-empty">Chưa có chỉnh sửa nào trên lớp này.</p>
                      ) : (
                        <div className="qgis-sub-list">
                          {historyItems.slice(0, 10).map((cs) => (
                            <div key={cs.id} className="qgis-sub-row">
                              <span>
                                <strong>{cs.summary}</strong> · {cs.actor_display_snapshot} ·{" "}
                                {cs.operation_count} thao tác · {new Date(cs.created_at).toLocaleString("vi-VN")}
                              </span>
                              <button
                                type="button"
                                className="qgis-btn-ghost"
                                disabled={saving}
                                onClick={() => { void handleUndoVersion(layer, cs); }}
                              >
                                ↩ Hoàn tác
                              </button>
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                  )}

                  {binding && conflictBindingId === binding.id && (
                    <div className="qgis-inline-drawer">
                      <div className="qgis-drawer-head">
                        <strong>Xử lý xung đột dữ liệu</strong>
                        <button type="button" className="qgis-btn-ghost" onClick={() => setConflictBindingId(null)}>Đóng</button>
                      </div>
                      {conflicts.length === 0 ? (
                        <p className="qgis-empty">Không có xung đột nào.</p>
                      ) : (
                        <div className="qgis-sub-list">
                          {conflicts.map((c) => (
                            <div key={c.id} className="qgis-conflict-box">
                              <div>
                                <strong>Đối tượng <code>{c.feature_id}</code>{c.field_name ? ` · ${c.field_name}` : ""}</strong>
                                <div className="qgis-conflict-vals">
                                  <span>Web: <code>{formatJsonValue(c.web_value)}</code></span>
                                  <span>QGIS: <code>{formatJsonValue(c.qgis_value)}</code></span>
                                </div>
                              </div>
                              {c.status === "open" && (
                                <div className="qgis-row-actions">
                                  <button type="button" className="qgis-btn-ghost" disabled={saving} onClick={() => { void handleResolveConflict(binding.id, c, "keep_web"); }}>Giữ Web</button>
                                  <button type="button" className="qgis-btn-ghost" disabled={saving} onClick={() => { void handleResolveConflict(binding.id, c, "keep_qgis"); }}>Giữ QGIS</button>
                                  <input
                                    type="text"
                                    placeholder="Giá trị gộp…"
                                    value={mergeDrafts[c.id] ?? ""}
                                    onChange={(e) => setMergeDrafts((p) => ({ ...p, [c.id]: e.target.value }))}
                                  />
                                  <button type="button" className="qgis-btn-primary is-sm" disabled={saving} onClick={() => { void handleResolveConflict(binding.id, c, "merge"); }}>Gộp</button>
                                </div>
                              )}
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              );
            })
          )}
        </div>
      )}
    </section>
  );
}