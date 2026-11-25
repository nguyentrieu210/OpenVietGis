import { useEffect, useState } from "react";
import { ApiError, createOfflinePackage, getCurrentUser, getSpatialFeature, getSpatialLayer,
  listOfflinePackages, listSpatialLayers, loadCsrfToken, syncOfflineOutbox,
  type OfflinePackage, type SpatialLayer } from "@openvietgis/client-sdk";
import { Link, useParams } from "react-router-dom";
import { apiClient } from "../../app/api";
import { OfflineMap } from "./OfflineMap";
import { activeOfflineActor, installRecord, listLocalPackages, lockLocalScope, lockOfflineActor, makeRecord,
  normalizeField, observeOnlineActor, queueDraft, removeRecord, saveRecord,
  unlockOfflineActor, verifyRecord, wireDrafts,
  type Draft, type PackageRecord, type Snapshot, type Bounds, type Value } from "./offline-store";
import "./offline.css";

const errorText = (err: unknown) => err instanceof Error ? err.message : "Thao tác thất bại.";
const path = (org: string, prj: string, pkg: string) =>
  `/organizations/${encodeURIComponent(org)}/projects/${encodeURIComponent(prj)}/field/packages/${encodeURIComponent(pkg)}`;

function DraftEditor({ snapshot, draft, id, save, disabled }: {
  snapshot: Snapshot; draft: Draft | undefined; id: string; save: (draft: Draft) => Promise<void>; disabled: boolean;
}) {
  const base = draft?.properties ?? snapshot.features.find(f => f.feature_id === id)?.properties ?? {};
  const [values, setValues] = useState<Record<string, string>>(Object.fromEntries(snapshot.fields.map(f => [f.field_id, String(base[f.field_id] ?? "")])));
  const [nulls, setNulls] = useState<Record<string, boolean>>(Object.fromEntries(snapshot.fields.map(f => [f.field_id, f.nullable && base[f.field_id] == null])));
  const [x, setX] = useState(draft?.geometry?.coordinates[0].toString() ?? "");
  const [y, setY] = useState(draft?.geometry?.coordinates[1].toString() ?? "");
  const [capture, setCapture] = useState<Draft["capture"]>(draft?.capture);
  const [error, setError] = useState("");
  const isNew = draft?.op_type === "create" || id === "new-point";
  function manual(axis: "x" | "y", val: string) {
    (axis === "x" ? setX : setY)(val); setCapture(undefined);
  }
  function locate() {
    setError("");
    if (snapshot.crs_code !== 4326 || !navigator.geolocation) {
      setError("Chỉ hỗ trợ vị trí thiết bị trong EPSG:4326."); return;
    }
    navigator.geolocation.getCurrentPosition(p => {
      setX(String(p.coords.longitude)); setY(String(p.coords.latitude));
      setCapture({ source: "device", time: new Date(p.timestamp).toISOString(), accuracy: Number.isFinite(p.coords.accuracy) ? p.coords.accuracy : null });
    }, () => setError("Không có quyền hoặc không lấy được vị trí thiết bị."), { enableHighAccuracy: false, maximumAge: 0, timeout: 15000 });
  }
  async function submit() {
    try {
      setError("");
      const properties: Record<string, Value> = {};
      for (const field of snapshot.fields) properties[field.field_id] = normalizeField(field, values[field.field_id] ?? "", Boolean(nulls[field.field_id]));
      if (isNew) {
        const xx = Number(x), yy = Number(y), b = snapshot.aoi_bbox;
        if (!x.trim() || !y.trim() || !Number.isFinite(xx) || !Number.isFinite(yy) ||
            xx < b.min_x || xx > b.max_x || yy < b.min_y || yy > b.max_y) throw new Error("Tọa độ nằm ngoài vùng được cấp.");
        await save({ op_type: "create", feature_id: draft?.feature_id ?? `field-${crypto.randomUUID()}`, properties,
          geometry: { type: "Point", coordinates: [xx, yy] },
          capture: capture ?? { source: "manual", time: new Date().toISOString(), accuracy: null } });
      } else await save({ op_type: "update", feature_id: id, properties });
    } catch (err) { setError(errorText(err)); }
  }
  return <form onSubmit={e => { e.preventDefault(); void submit(); }} className="offline-form">
    <h3>{isNew ? "Tạo điểm mới · bản nháp" : "Sửa thuộc tính · " + id}</h3>
    {isNew && <>
      <label>X · EPSG:{snapshot.crs_code}<input aria-label="Tọa độ X" value={x} onChange={e => manual("x", e.target.value)} disabled={disabled}/></label>
      <label>Y · EPSG:{snapshot.crs_code}<input aria-label="Tọa độ Y" value={y} onChange={e => manual("y", e.target.value)} disabled={disabled}/></label>
      {snapshot.crs_code === 4326 && <button type="button" onClick={locate} disabled={disabled}>Yêu cầu vị trí thiết bị</button>}
      <p>Nguồn: {capture?.source === "device" ? "thiết bị" : "nhập tay"} · {capture?.time ?? "thời điểm lưu"} · độ chính xác: {capture?.accuracy != null ? capture.accuracy + " m" : "không rõ"}. Không phải tọa độ khảo sát.</p>
    </>}
    {snapshot.fields.map(field => <div key={field.field_id}>
      <label>{field.name} ({field.type}){field.type === "boolean"
        ? <select value={values[field.field_id] ?? ""} onChange={e => setValues(v => ({...v, [field.field_id]: e.target.value}))} disabled={disabled || nulls[field.field_id]}><option value="">--</option><option value="true">Có</option><option value="false">Không</option></select>
        : <input value={values[field.field_id] ?? ""} onChange={e => setValues(v => ({...v, [field.field_id]: e.target.value}))} disabled={disabled || nulls[field.field_id]}/>}</label>
      {field.nullable && <label><input type="checkbox" checked={nulls[field.field_id] ?? false} onChange={e => setNulls(n => ({...n, [field.field_id]: e.target.checked}))} disabled={disabled}/> NULL</label>}
    </div>)}
    <button type="submit" disabled={disabled}>{isNew ? "Xác nhận tọa độ và lưu bản nháp" : "Lưu bản nháp ngoại tuyến"}</button>
    {error && <p role="alert">{error}</p>}
  </form>;
}

function OpenPackage({ record, snapshot, online, save, sync, disabled }: {
  record: PackageRecord; snapshot: Snapshot; online: boolean; save: (record: PackageRecord) => Promise<void>;
  sync: () => Promise<void>; disabled: boolean;
}) {
  const [selection, setSelection] = useState("");
  const [compare, setCompare] = useState<unknown>(null);
  const draft = record.drafts.find(d => d.feature_id === selection);
  const baseline = snapshot.features.find(f => f.feature_id === selection);
  const canEdit = !disabled && !record.batch?.attempted;
  async function compareCurrent() {
    try {
      setCompare(null);
      const l = await getSpatialLayer(apiClient, snapshot.organization_id, snapshot.project_id, snapshot.layer_id);
      const f = await getSpatialFeature(apiClient, snapshot.organization_id, snapshot.project_id, snapshot.layer_id, selection,
        {content_revision: l.source.content_revision, schema_revision: l.source.schema_revision});
      setCompare(f.feature);
    } catch { setCompare("Không được phép xem hoặc đối tượng không còn tồn tại."); }
  }
  return <>
    <h2>{record.name} · {snapshot.layer_name}</h2>
    <p>Đã chụp: {snapshot.created_at} | Hết hạn: {snapshot.expires_at} | EPSG:{snapshot.crs_code}<br/>Phiên bản dữ liệu: <code>{snapshot.content_revision}</code></p>
    <OfflineMap bounds={snapshot.aoi_bbox} features={snapshot.features} selection={selection} select={id => {setSelection(id); setCompare(null);}}/>
    <label>Chọn đối tượng
      <select value={selection} onChange={e => {setSelection(e.target.value); setCompare(null);}}>
        <option value="">Chọn đối tượng</option>
        {snapshot.features.map(f => <option key={f.feature_id} value={f.feature_id}>{f.feature_id}</option>)}
        {record.drafts.filter(d => d.op_type === "create").map(d => <option key={d.feature_id} value={d.feature_id}>Nháp: {d.feature_id}</option>)}
      </select>
    </label>
    {snapshot.geometry_family === "Point" && <button type="button" disabled={!canEdit} onClick={() => {setSelection("new-point");setCompare(null);}}>Điểm mới</button>}
    {baseline && <details><summary>Thuộc tính gốc (chưa thay đổi)</summary><pre>{JSON.stringify(baseline.properties, null, 2)}</pre></details>}
    {selection && (baseline || draft || selection === "new-point") && <DraftEditor key={selection + record.revision} id={selection} snapshot={snapshot} draft={draft}
      disabled={!canEdit} save={async d => { await save(queueDraft(record, d)); setSelection(d.feature_id); }}/>}
    <h3>Hàng đợi bản nháp · {record.drafts.length}</h3>
    <p>Bản nháp không phải dữ liệu đã xuất bản. Tất cả thay đổi dùng cùng mốc phiên bản.</p>
    <ul>{record.drafts.map(d => {
      const result = record.batch?.receipt?.operation_receipts?.find(r => r.client_operation_id === d.client_operation_id);
      return <li key={d.feature_id}>
      {d.op_type === "create" ? "Tạo điểm" : "Sửa thuộc tính"} — {d.feature_id} — {record.batch?.receipt?.status ?? (record.batch?.attempted ? "đã gửi, chờ xác nhận" : "đang chờ")}
      {result && <span> · Core: {result.core_feature_id ?? "Không xuất bản"}</span>}
      <button type="button" onClick={() => {setSelection(d.feature_id);setCompare(null);}}>Xem</button>
      {canEdit && <button type="button" onClick={() => {if (window.confirm("Bỏ bản nháp này?")) void save({...record, drafts: record.drafts.filter(x => x.feature_id !== d.feature_id)});}}>Bỏ nháp</button>}
    </li>;})}</ul>
    <button type="button" disabled={!online || disabled || !record.drafts.length || Boolean(record.batch?.receipt)} onClick={() => {if (window.confirm("Gửi toàn bộ nhóm thay đổi để máy chủ kiểm tra quyền và phiên bản?")) void sync();}}>
      {record.batch?.attempted ? "Thử lại đúng nhóm đã gửi" : "Đồng bộ nhóm bản nháp"}
    </button>
    {record.batch?.receipt && <section data-testid="offline-receipt"><h3>Kết quả đồng bộ đã lưu</h3>
      <p>{record.batch.receipt.status === "applied" ? "Đã chấp nhận toàn bộ nhóm" : "Nhóm không được xuất bản; cần review rõ ràng."}</p>
      <pre>{JSON.stringify(record.batch.receipt, null, 2)}</pre>
      {record.batch.receipt.status !== "applied" && <>
        <p>Cần giữ bản nháp cũ để đối chiếu. Chỉ tạo bản nháp mới trên gói mới được máy chủ cấp; không tự đổi phiên bản của nhóm đã gửi.</p>
        {baseline && <button type="button" disabled={!online || disabled} onClick={() => void compareCurrent()}>Đối chiếu dữ liệu hiện tại</button>}
        {compare != null && <pre>{JSON.stringify(compare, null, 2)}</pre>}
      </>}
    </section>}
  </>;
}

export function OfflineFieldPage() {
  const { organizationId: org = "", projectId: prj = "" } = useParams();
  const [online, setOnline] = useState(navigator.onLine);
  const [shellReady, setShellReady] = useState(Boolean(navigator.serviceWorker?.controller));
  const [actor, setActor] = useState<string | null>(null);
  const [records, setRecords] = useState<PackageRecord[]>([]);
  const [server, setServer] = useState<OfflinePackage[]>([]);
  const [layers, setLayers] = useState<SpatialLayer[]>([]);
  const [active, setActive] = useState("");
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [layerId, setLayerId] = useState("");
  const [name, setName] = useState("Gói hiện trường");
  const [ttl, setTtl] = useState("48");
  const [bbox, setBbox] = useState<Record<keyof Bounds, string>>({min_x:"",min_y:"",max_x:"",max_y:""});
  const [consent, setConsent] = useState(false);
  const [now, setNow] = useState(Date.now());
  const record = records.find(r => r.id === active);
  const chosen = layers.find(l => l.id === layerId);

  useEffect(() => {
    const status = () => {setOnline(navigator.onLine); setRefresh(n => n + 1);};
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    window.addEventListener("online", status); window.addEventListener("offline", status);
    window.addEventListener("storage", status);
    const onFocus = () => {if (navigator.onLine) setRefresh(n => n + 1);};
    window.addEventListener("focus", onFocus);
    const checkShell = () => setShellReady(Boolean(navigator.serviceWorker?.controller));
    navigator.serviceWorker?.addEventListener("controllerchange", checkShell);
    return () => {clearInterval(timer);window.removeEventListener("online", status);window.removeEventListener("offline", status);window.removeEventListener("storage", status);window.removeEventListener("focus", onFocus);navigator.serviceWorker?.removeEventListener("controllerchange", checkShell);};
  }, []);
  useEffect(() => {
    let live = true;
    setActor(null); setSnapshot(null); setRecords([]); setServer([]); setLayers([]);
    async function load() {
      let actorId: string | null = activeOfflineActor();
      try {
        let packages: OfflinePackage[] = [];
        let layerRows: SpatialLayer[] = [];
        if (online) {
          const user = await getCurrentUser(apiClient);
          if (!live) return;
          observeOnlineActor(user.id);
          actorId = user.status === "active" ? user.id : null;
          if (!actorId) { lockOfflineActor(); return; }
          const [p,l] = await Promise.all([listOfflinePackages(apiClient, org, prj), listSpatialLayers(apiClient, org, prj, {limit:100})]);
          if (!live) return;
          packages = p.items; layerRows = l.items.filter(layer => layer.source.kind !== "reference_fixture" && layer.source.validation_state === "ready" && layer.source.lifecycle === "active" && layer.capabilities.some(c => c.name === "edit" && c.available));
        } else actorId = activeOfflineActor();
        if (!actorId) throw new Error("Thiết bị chưa được mở khóa ngoại tuyến bởi tài khoản hợp lệ.");
        let local = await listLocalPackages(actorId, org, prj);
        if (!live) return;
        if (online) {
          local = await Promise.all(local.map(r => {
            const p = packages.find(item => item.id === r.id);
            return !r.locked && (!p || p.revoked_at || p.sha256 !== r.gpkgSha) ? saveRecord({...r, locked:true}) : Promise.resolve(r);
          }));
        }
        if (!live) return;
        setRecords(local); setServer(packages); setLayers(layerRows); setActor(actorId);
        setError("");
      } catch (err) {
        if (!live) return;
        if (err instanceof ApiError && [401,403,404].includes(err.status)) {
          lockOfflineActor();
          if (actorId && [403,404].includes(err.status)) {
            try { await lockLocalScope(actorId, org, prj); }
            catch (lockErr) { if (live) setError(errorText(lockErr)); return; }
          }
        }
        if (live) setError(errorText(err));
      }
    }
    void load();
    return () => {live = false;};
  }, [online, refresh, org, prj]);

  useEffect(() => {
    let live = true;
    setSnapshot(null);
    if (record && actor === record.actorId) void verifyRecord(record).then(s => { if(live) setSnapshot(s); })
      .catch(err => {if(live) setError(errorText(err));});
    return () => {live = false;};
  }, [record, actor]);

  function ensureActor(): string {
    if (!actor) throw new Error("Cần xác thực tài khoản.");
    if (!online && activeOfflineActor() !== actor) throw new Error("Phiên ngoại tuyến đã khóa.");
    return actor;
  }
  async function verifyOnlineActor() {
    const expected = ensureActor();
    const user = await getCurrentUser(apiClient);
    if (user.id !== expected || user.status !== "active") {
      lockOfflineActor(); setActor(null); setSnapshot(null); throw new Error("Tài khoản đã thay đổi; dữ liệu được khóa.");
    }
    return expected;
  }
  async function persist(next: PackageRecord) {
    if (ensureActor() !== next.actorId) throw new Error("Gói không thuộc tài khoản.");
    const updated = await saveRecord(next);
    setRecords(rows => rows.map(r => r.id === updated.id ? updated : r));
  }
  async function download(pkg: OfflinePackage, actorId: string) {
    if (!shellReady || !window.isSecureContext) throw new Error("Cần bản production HTTPS hoặc localhost với shell ngoại tuyến sẵn sàng.");
    if (records.length >= 5) throw new Error("Tối đa 5 gói trong dự án trên thiết bị; hãy xóa gói không dùng.");
    const base = path(org, prj, pkg.id);
    const [gpkg, projection] = await Promise.all([
      apiClient.requestRaw(base + "/download", {cache:"no-store"}),
      apiClient.requestRaw(base + "/snapshot", {cache:"no-store"})
    ]);
    const verified = await makeRecord(pkg, await projection.arrayBuffer(), await gpkg.arrayBuffer(), actorId, org, prj);
    await verifyOnlineActor();
    const installed = await installRecord(verified);
    unlockOfflineActor(actorId);
    setRecords(rows => [...rows, installed]); setActive(pkg.id);
    setNotice("Gói đã kiểm tra checksum và lưu trên thiết bị.");
  }
  async function withBusy(fn: () => Promise<void>) {
    setBusy(true); setError(""); setNotice("");
    try {await fn();} catch (err) {setError(errorText(err));} finally {setBusy(false);}
  }

  async function issue() {
    await withBusy(async () => {
      if (!consent || !chosen) throw new Error("Hãy chọn lớp và xác nhận chính sách lưu trên thiết bị.");
      const actorId = await verifyOnlineActor();
      const coords = Object.fromEntries(Object.entries(bbox).map(([k,v]) => [k, v.trim() ? Number(v) : NaN])) as Bounds;
      if (!Object.values(coords).every(Number.isFinite) || coords.min_x >= coords.max_x || coords.min_y >= coords.max_y) throw new Error("Vùng dữ liệu không hợp lệ.");
      const csrf = await loadCsrfToken(apiClient);
      const pkg = await createOfflinePackage(apiClient, org, prj, {name, layer_id: layerId, aoi_bbox:coords, ttl_hours:Number(ttl),local_storage_confirmed:true}, csrf);
      setServer(rows => [pkg,...rows]);
      await download(pkg, actorId);
    });
  }

  async function sync() {
    if (!record || !record.batch || record.batch.receipt) return;
    await withBusy(async () => {
      await verifyRecord(record);
      await verifyOnlineActor();
      // Commit attempted state first, so retry after shutdown uses identical group ID/content.
      let pending = record;
      if (!record.batch?.attempted) {
        if (!record.batch) throw new Error("Không có nhóm bản nháp.");
        pending = await saveRecord({...record, batch:{...record.batch, attempted:true}});
        setRecords(rows => rows.map(r => r.id === pending.id ? pending : r));
      }
      if (!pending.batch) throw new Error("Không có nhóm bản nháp.");
      try {
        const csrf = await loadCsrfToken(apiClient);
        const receipt = await syncOfflineOutbox(apiClient, org, prj, pending.id, {operation_id:pending.batch.id,operations:wireDrafts(pending.drafts)}, csrf);
        const completed = {...pending, batch:{...pending.batch, receipt}};
        await persist(completed);
        setNotice("Đã lưu kết quả máy chủ; bản nháp được giữ để đối chiếu.");
      } catch (err) {
        if (err instanceof ApiError && [401,403,404].includes(err.status)) {
          lockOfflineActor(); setActor(null); setSnapshot(null); setRecords([]); setServer([]); setLayers([]);
          if ([403,404].includes(err.status)) await lockLocalScope(pending.actorId, org, prj);
        } else if (err instanceof ApiError && [409,422].includes(err.status)) {
          await persist({...pending,batch:{...pending.batch, receipt:{id:"",status:"rejected",applied_changeset_ids:[],conflicts:["Máy chủ từ chối; hãy kiểm tra lại phạm vi và phiên bản."]}}});
        }
        throw err;
      }
    });
  }

  return <main className="offline-page" data-testid="offline-field-page">
    <h1>R013 Hiện trường ngoại tuyến</h1>
    <p>Trạng thái: <strong data-testid="offline-connectivity">{online ? "Trực tuyến" : "Mất kết nối"}</strong> · {actor ? "Tài khoản đã xác minh/mở khóa" : "Đang khóa dữ liệu"} · Shell ngoại tuyến: <strong data-testid="offline-shell">{shellReady ? "đã sẵn sàng" : "chưa sẵn sàng"}</strong></p>
    <p>Gói chỉ dành cho dữ liệu được phép lưu cục bộ trên thiết bị tin cậy. Không có mã hóa bổ sung hoặc xóa từ xa; hạn dùng dựa vào đồng hồ thiết bị. Thu hồi trên máy chủ được kiểm tra khi kết nối lại.</p>
    {online && <p><Link to={`/app/organizations/${org}/projects/${prj}/platform-hub`}>Về dự án</Link> · <button onClick={() => setRefresh(n => n+1)}>Kiểm tra lại quyền và gói</button></p>}
    {error && <p role="alert">{error}</p>}{notice && <p role="status">{notice}</p>}
    {online && actor && <section className="offline-form">
      <h2>Chuẩn bị gói</h2>
      <p>Tối đa 5000 đối tượng, 14 ngày, dữ liệu nội bộ có quyền sửa; chỉ thuộc tính và điểm mới. Hệ tọa độ gốc, không tự chuyển đổi.</p>
      <label>Tên gói<input value={name} onChange={e=>setName(e.target.value)}/></label>
      <label>Lớp được cấp quyền<select value={layerId} onChange={e=>{setLayerId(e.target.value);setBbox({min_x:"",min_y:"",max_x:"",max_y:""});}}>
        <option value="">Chọn lớp</option>{layers.map(l=><option key={l.id} value={l.id}>{l.name} — EPSG:{l.source.stored_crs?.code} ({l.source.feature_count ?? "?"} đối tượng toàn lớp)</option>)}
      </select></label>
      {chosen && <p>EPSG:{chosen.source.stored_crs?.code} · Phiên bản {chosen.source.content_revision}. Chỉ đối tượng nằm hoàn toàn trong vùng chọn mới được bao gồm; đối tượng giao ranh giới bị loại, không cắt hình học. Lớp không hỗ trợ không được đóng gói.</p>}
      {chosen?.source.extent && <button type="button" onClick={() => {
        const e = chosen.source.extent;
        if(e) setBbox({min_x:String(e.min_x),min_y:String(e.min_y),max_x:String(e.max_x),max_y:String(e.max_y)});
      }}>Dùng biên lớp (có thể chỉnh)</button>}
      {(["min_x","min_y","max_x","max_y"] as const).map(k=><label key={k}>{k}<input value={bbox[k]} onChange={e => setBbox(b=>({...b,[k]:e.target.value}))}/></label>)}
      <label>Giờ hiệu lực (1–336)<input type="number" min="1" max="336" value={ttl} onChange={e=>setTtl(e.target.value)}/></label>
      <label><input type="checkbox" checked={consent} onChange={e=>setConsent(e.target.checked)}/> Tôi xác nhận lớp không chứa dữ liệu nhạy cảm/bảo vệ và được phép lưu trên thiết bị này.</label>
      <button disabled={busy || !consent || !chosen || !shellReady} type="button" onClick={()=>void issue()}>Tạo & tải gói đã xác minh</button>
      <h3>Gói đã cấp chưa tải</h3>
      <ul>{server.filter(p=>!records.some(r=>r.id===p.id) && !p.revoked_at && Date.parse(p.expires_at)>now).map(p=><li key={p.id}>{p.name} | {p.feature_count} đối tượng | {p.byte_size} byte
        <button type="button" disabled={busy || !consent || !shellReady} onClick={()=>void withBusy(async()=>download(p,await verifyOnlineActor()))}>Tải & xác minh</button>
      </li>)}</ul>
    </section>}
    {actor && <section>
      <h2>Gói lưu trên thiết bị</h2>
      {online && shellReady && <button type="button" disabled={busy} onClick={()=>void withBusy(async()=>{
        if (!window.confirm("Mở khóa các gói còn hiệu lực cho tài khoản này khi mất mạng?")) return;
        unlockOfflineActor(await verifyOnlineActor()); setNotice("Thiết bị đã mở khóa ngoại tuyến.");
      })}>Mở khóa ngoại tuyến cho tài khoản này</button>}
      {!records.length && <p>Chưa có gói được tải.</p>}
      <ul>{records.map(r=><li key={r.id}><strong>{r.name}</strong> · {r.locked ? "Bị khóa" : Date.parse(r.expiresAt)<=now ? "Hết hạn" : "Đã tải (kiểm tra khi mở)"} · {r.drafts.length} nháp
        <button type="button" disabled={busy || r.locked || Date.parse(r.expiresAt)<=now} onClick={()=>setActive(r.id)}>Mở gói</button>
        <button type="button" disabled={busy} onClick={()=>void withBusy(async()=>{
          if(!window.confirm("Xóa gói cục bộ? " + (r.drafts.length ? "Bản nháp chưa đồng bộ có thể không khôi phục được." : ""))) return;
          ensureActor(); await removeRecord(r); setRecords(rows=>rows.filter(x=>x.id!==r.id)); if(active===r.id)setActive("");
        })}>Xóa gói</button>
      </li>)}</ul>
      <button type="button" onClick={()=>{lockOfflineActor();setActor(null);setSnapshot(null);setRecords([]);}}>Khóa thiết bị ngoại tuyến</button>
    </section>}
    {record && snapshot && actor === record.actorId && !record.locked && Date.parse(record.expiresAt)>now &&
      <OpenPackage key={record.id} record={record} snapshot={snapshot} online={online} disabled={busy}
        save={async next => {
          if(Date.parse(next.expiresAt)<=Date.now()) throw new Error("Gói hết hạn.");
          try{await persist(next);}catch(err){setError(errorText(err));throw err;}
        }} sync={sync}/>}
  </main>;
}
