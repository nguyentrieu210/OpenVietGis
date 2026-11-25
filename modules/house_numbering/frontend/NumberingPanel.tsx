import {uiConfirm} from "../../../apps/web-next/src/ui-dialog";
import {
  createHouseNumberingRule,
  inspectHouseNumberingLocation,
  listNumberingTargets,
  type NumberingTarget,
  listHouseNumberingRoutes,
  listHouseNumberingRules,
  previewHouseNumbering,
  applyHouseNumberingPreview,
  type HouseNumberingPreview
} from "./api";
import { useState } from "react";
import { hnClient, uiError, withCsrf } from "./client";
import { useReloadable } from "./hooks";

type Props = { organizationId: string; projectId: string };

export function NumberingPanel({ organizationId, projectId }: Props) {
  const rules = useReloadable(
    () => listHouseNumberingRules(hnClient, organizationId, projectId),
    [organizationId, projectId]
  );
  const routes = useReloadable(
    () => listHouseNumberingRoutes(hnClient, organizationId, projectId),
    [organizationId, projectId]
  );
  const [scopeRoute, setScopeRoute] = useState("");
  const [descendants, setDescendants] = useState(true);
  const [targets, setTargets] = useState<NumberingTarget[]>([]);
  const [total, setTotal] = useState(0);
  const [truncated, setTruncated] = useState(false);
  const [page, setPage] = useState(0);
  const [busy, setBusy] = useState(false);
  const routeNames = new Map((routes.data?.items ?? []).map(r => [r.id,r.name]));
  const [routeId, setRouteId] = useState("");
  const [ruleId, setRuleId] = useState("");
  const [x, setX] = useState("0");
  const [y, setY] = useState("0");
  const [crs, setCrs] = useState("5899");
  const [inspection, setInspection] = useState<Record<string, unknown> | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [preview, setPreview] = useState<HouseNumberingPreview | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [ruleVersion, setRuleVersion] = useState("QD-HN-2026");
  const [authority, setAuthority] = useState("");

  async function mutate(action: () => Promise<void>) {
    setError(null); setNotice(null); setBusy(true);
    try { await withCsrf(action); } catch (err) { setError(await uiError(err)); }
    finally { setBusy(false); }
  }

  function resetScope() {
    setTargets([]); setTotal(0); setTruncated(false); setPage(0); setSelected([]); setPreview(null);
  }
  async function loadTargets() {
    resetScope();
    const result = await listNumberingTargets(hnClient, organizationId, projectId, scopeRoute, descendants);
    setTargets(result.items); setTotal(result.total); setTruncated(result.truncated);
    setSelected(result.items.map(b => b.id));
    setNotice(`Đã chọn ${result.items.length} nhà đủ điều kiện trong cây tuyến. Chưa ghi số.`);
  }

  return <>
    <section className="hn-card">
      <h2>Quy tắc đánh số</h2>
      <div className="hn-form">
        <label>Phiên bản<input value={ruleVersion} onChange={(e) => setRuleVersion(e.target.value)} /></label>
        <label>Căn cứ pháp lý<input value={authority} onChange={(e) => setAuthority(e.target.value)} placeholder="Số quyết định / văn bản" /></label>
        <button className="hn-btn" type="button" onClick={() => void mutate(async () => {
          await createHouseNumberingRule(hnClient, organizationId, projectId, {
            version: ruleVersion, status: authority.trim() ? "authoritative" : "draft",
            authority_ref: authority.trim() || null, left_odd: true,
            suffix_style: "letters", max_distance_m: 150
          });
          setNotice("Đã tạo phiên bản quy tắc."); await rules.reload();
        })}>Tạo quy tắc</button>
      </div>
      <p className="hn-muted">Chỉ rule có căn cứ authoritative mới cho phép duyệt chính thức.</p>
    </section>

    <section className="hn-card">
      <h2>Kiểm tra vị trí trên tuyến</h2>
      <div className="hn-form">
        <label>Tuyến<select value={routeId} onChange={(e) => setRouteId(e.target.value)}><option value="">Chọn tuyến</option>{(routes.data?.items ?? []).map((r) => <option key={r.id} value={r.id}>{r.name}</option>)}</select></label>
        <label>Rule<select value={ruleId} disabled={busy} onChange={(e) => {setRuleId(e.target.value); setPreview(null);}}><option value="">Chọn rule</option>{(rules.data?.items ?? []).map((r) => <option key={r.id} value={r.id}>{r.version} · {r.status}</option>)}</select></label>
        <label>X<input value={x} onChange={(e) => setX(e.target.value)} /></label>
        <label>Y<input value={y} onChange={(e) => setY(e.target.value)} /></label>
        <label>EPSG<input value={crs} onChange={(e) => setCrs(e.target.value)} /></label>
        <button className="hn-btn" disabled={!routeId || !ruleId} type="button" onClick={() => void mutate(async () => {
          const result = await inspectHouseNumberingLocation(hnClient, organizationId, projectId, {
            x: Number(x), y: Number(y), crs_code: Number(crs), route_id: routeId, rule_id: ruleId
          });
          setInspection(result as unknown as Record<string, unknown>);
        })}>Kiểm tra</button>
      </div>
      {inspection && <pre className="hn-code">{JSON.stringify(inspection, null, 2)}</pre>}
    </section>

    <section className="hn-card">
      <h2>Đánh số hàng loạt theo cây tuyến</h2>
      <p className="hn-muted">Chọn tuyến cha để xử lý cả cây tuyến con. Giữ nguyên số đã đề xuất hoặc được cấp; nhà chưa liên kết tuyến không được đánh số.</p>
      <div className="hn-form">
        <label>Tuyến gốc<select disabled={busy} value={scopeRoute} onChange={e => {setScopeRoute(e.target.value); resetScope();}}>
          <option value="">Chọn tuyến</option>{(routes.data?.items ?? []).map(r => <option key={r.id} value={r.id}>{r.name} {r.prefix ? `· ${r.prefix}` : ""}</option>)}
        </select></label>
        <label><input type="checkbox" disabled={busy} checked={descendants} onChange={e => {setDescendants(e.target.checked); resetScope();}}/> Bao gồm tuyến con</label>
        <button className="hn-btn" type="button" disabled={!scopeRoute || busy} onClick={() => void mutate(loadTargets)}>Tải và chọn nhà chưa đánh số</button>
      </div>
      <p>Đã chọn {selected.length.toLocaleString()} / {total.toLocaleString()} nhà.</p>
      {truncated && <p className="hn-warning">Giới hạn mỗi lô 5.000 nhà. Sau khi lưu, tải lại để tiếp tục lô kế tiếp hoặc chọn tuyến con.</p>}
      {!!targets.length && <>
        <div className="hn-form">
          <button className="hn-btn" type="button" disabled={busy} onClick={() => {setSelected(targets.map(b=>b.id)); setPreview(null);}}>Chọn tất cả trong lô</button>
          <button className="hn-btn" type="button" disabled={busy} onClick={() => {setSelected([]);setPreview(null);}}>Bỏ chọn</button>
          <button className="hn-btn" type="button" disabled={page===0 || busy} onClick={() => setPage(p=>p-1)}>Trang trước</button>
          <span>Trang {page+1}/{Math.ceil(targets.length/50)}</span>
          <button className="hn-btn" type="button" disabled={(page+1)*50>=targets.length || busy} onClick={() => setPage(p=>p+1)}>Trang sau</button>
        </div>
        <div className="hn-checks">{targets.slice(page*50,(page+1)*50).map(b=><label key={b.id}>
          <input type="checkbox" disabled={busy} checked={selected.includes(b.id)} onChange={e=>{setPreview(null);setSelected(current=>e.target.checked?[...current,b.id]:current.filter(id=>id!==b.id));}}/>
          {b.parcel_ref ?? b.id.slice(0,8)} · {routeNames.get(b.route_id ?? "") ?? "Tuyến"} · {b.eligibility}
        </label>)}</div>
      </>}
      {!ruleId && <p className="hn-warning">Chọn quy tắc ở phần Kiểm tra vị trí trên tuyến trước khi lập phương án.</p>}
      <div className="hn-form">
        <button className="hn-btn" type="button" disabled={!ruleId || !selected.length || busy} onClick={()=>void mutate(async()=>{
          setPreview(null);
          const result=await previewHouseNumbering(hnClient,organizationId,projectId,{building_ids:selected,rule_id:ruleId,operation_id:crypto.randomUUID()});
          setPreview(result);setNotice(`Phương án gồm ${result.candidate_changes.length} số đề xuất và ${result.warnings?.length ?? 0} nhà cần rà soát. Chưa ghi dữ liệu.`);
        })}>Lập phương án đánh số</button>
        <button className="hn-btn" type="button" disabled={!preview || !preview.candidate_changes.length || !!preview.applied_at || busy} onClick={async ()=>{
          if(!preview || !(await uiConfirm(`Lưu ${preview.candidate_changes.length} số đề xuất? Đây chưa phải cấp số chính thức.`))) return;
          void mutate(async()=>{
            await applyHouseNumberingPreview(hnClient,organizationId,projectId,preview.id);
            setPreview({...preview,applied_at:new Date().toISOString()});
            setNotice("Đã lưu số đề xuất. Tải lại cây tuyến để tiếp tục.");
          });
        }}>Xác nhận lưu số đề xuất</button>
      </div>
      {busy && <p>Đang xử lý…</p>}
      {preview?.audit && <section className="hn-card" aria-label="Kiểm toán phương án đánh số">
        <h3>Kiểm toán số nhà theo toàn bộ cây tuyến</h3>
        <p className="hn-muted">Kiểm tra riêng từng tuyến nghiệp vụ, theo chiều vẽ và lý trình thực tế. Số trùng, sai chẵn/lẻ, sai tiền tố và đảo thứ tự sẽ khóa đề xuất của tuyến lỗi. Khoảng số chưa dùng chỉ là thông tin tham khảo, không tự động cấp bù.</p>
        <div className="hn-actions" style={{flexWrap:"wrap"}}>
          <span className="hn-badge">{preview.audit.routes} tuyến kiểm tra</span>
          <span className="hn-badge">{preview.audit.existing} số đang có</span>
          <span className="hn-badge">{preview.audit.duplicate_count} số trùng trong tuyến</span>
          <span className="hn-badge">{preview.audit.cross_route_duplicate_count ?? 0} số trùng giữa tuyến cùng đường</span>
          <span className="hn-badge">{preview.audit.parity_error_count} sai chẵn/lẻ</span>
          <span className="hn-badge">{preview.audit.ordering_error_count} sai thứ tự</span>
          <span className="hn-badge">{preview.audit.invalid_format_count} sai định dạng</span>
          <span className="hn-badge">{preview.audit.gap_count} vị trí số còn trống</span>
        </div>
        {preview.audit.blocking_issues > 0 ? <p className="hn-warning">Có {preview.audit.blocking_issues} điểm cần sửa trong dữ liệu hiện có. Những tuyến lỗi đã bị loại khỏi danh sách số được phép lưu. Chỉ các tuyến còn hợp lệ mới có số đề xuất.</p> : <p className="hn-ok">Không phát hiện số trùng, sai chẵn/lẻ hoặc sai thứ tự trong các tuyến đã kiểm tra.</p>}
        <div style={{maxHeight:360,overflow:"auto"}}>{preview.audit.details.map(a=><details key={a.route_id} style={{borderBottom:"1px solid var(--ovg-border,#dbe3ec)",padding:6}}>
          <summary><b>{a.route_name}</b> · {a.proposed} đề xuất thử · {a.blocking_issues} lỗi chặn · {a.gap_count} khoảng số trống</summary>
          {a.duplicates.map(d=><p key={"dup"+d.number}>Số trùng <b>{d.number}</b> · {d.occupants.length} hồ sơ</p>)}
          {a.cross_route_duplicates?.map(d=><p key={"cross"+d.root_route_id+d.number}>Trùng giữa các tuyến cùng đường: <b>{d.number}</b> · {new Set(d.occupants.map(x=>x.route_id)).size} tuyến</p>)}
          {a.parity_errors.map((x,i)=><p key={"par"+i}>Sai chẵn/lẻ: {x.number} · bên {x.side==="left"?"trái":"phải"}</p>)}
          {a.invalid_formats.map((x,i)=><p key={"format"+i}>Sai tiền tố/định dạng: {x.number}</p>)}
          {a.ordering_errors.map((x,i)=><p key={"order"+i}>Sai thứ tự dọc tuyến: {x.before} → {x.after} · bên {x.side==="left"?"trái":"phải"}</p>)}
          {a.gaps.map((x,i)=><p key={"gap"+i}>Khoảng số chưa dùng: {x.from_number} → {x.to_number} · {x.unused_slots} số {x.side==="left"?"bên trái":"bên phải"}</p>)}
          {a.blocking_issues===0&&a.gap_count===0&&<p>Không phát hiện bất thường trên tuyến này.</p>}
        </details>)}</div>
      </section>}
      {preview && <><p>{preview.candidate_changes.length} số đề xuất hợp lệ · hiển thị tối đa 200 dòng.</p><div style={{maxHeight:"460px",overflow:"auto"}}><table className="hn-table"><thead><tr><th>Nhà</th><th>Tuyến</th><th>Số cũ</th><th>Số đề xuất</th><th>Bên / lý trình</th><th>Lý do</th></tr></thead><tbody>{preview.candidate_changes.slice(0,200).map(c=><tr key={c.building_id}><td>{c.building_id.slice(0,8)}</td><td>{routeNames.get(c.route_id) ?? c.route_id.slice(0,8)}</td><td>{c.old_number ?? "—"}</td><td><strong>{c.number}</strong></td><td>{c.side ?? "?"} / {c.measure.toFixed(1)}m</td><td>{c.reason}</td></tr>)}</tbody></table></div></>}
      {preview?.warnings?.length ? <details className="hn-warning"><summary>{preview.warnings.length} nhà cần rà soát (tối đa 200 hiển thị)</summary><ul>{preview.warnings.slice(0,200).map((w,i)=><li key={i}>{w}</li>)}</ul></details> : null}
      {notice && <div className="hn-ok">{notice}</div>}{error && <div className="hn-error">{error}</div>}
    </section>
  </>;
}
