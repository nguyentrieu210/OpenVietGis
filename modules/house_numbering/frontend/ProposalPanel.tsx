import {
  createHouseNumberingProposal,
  getHouseNumberingProposal,
  listHouseNumberingProposals,
  houseNumberingProposalPacketUrl,
  listHouseNumberingBuildingLineage,
  listHouseNumberingBuildings,
  listHouseNumberingRoutes,
  listHouseNumberingRules,
  recordHouseNumberingBuildingLineage,
  setHouseNumberingPlaqueStatus,
  transitionHouseNumberingProposal,
  type HouseNumberingBuilding,
  type HouseNumberingBuildingLineage,
  type HouseNumberingProposal
} from "./api";
import { useState } from "react";
import { hnClient, uiError, withCsrf } from "./client";
import { useReloadable } from "./hooks";

type Props = { organizationId: string; projectId: string };

export function ProposalPanel({ organizationId, projectId }: Props) {
  const rules = useReloadable(
    () => listHouseNumberingRules(hnClient, organizationId, projectId),
    [organizationId, projectId]
  );
  const routes = useReloadable(
    () => listHouseNumberingRoutes(hnClient, organizationId, projectId),
    [organizationId, projectId]
  );
  const buildings = useReloadable(
    () => listHouseNumberingBuildings(hnClient, organizationId, projectId),
    [organizationId, projectId]
  );

  const [queueStatus,setQueueStatus] = useState<"all"|"draft"|"submitted"|"approved">("all");
  const [queueOffset,setQueueOffset] = useState(0);
  const [qrOffset,setQrOffset] = useState(0);
  const queue=useReloadable(()=>listHouseNumberingProposals(hnClient,organizationId,projectId,{
    ...(queueStatus==="all"?{}:{status:queueStatus}),offset:queueOffset,limit:100
  }),[organizationId,projectId,queueStatus,queueOffset]);

  const [routeId, setRouteId] = useState("");
  const [ruleId, setRuleId] = useState("");
  const [selected, setSelected] = useState<string[]>([]);
  const [proposal, setProposal] = useState<HouseNumberingProposal | null>(null);
  const [comment, setComment] = useState("");

  const [plaqueBuildingId, setPlaqueBuildingId] = useState("");
  const [plaqueStatus, setPlaqueStatus] =
    useState<HouseNumberingBuilding["plaque_status"]>("planned");

  const [lineageKind, setLineageKind] = useState<"split" | "merge">("split");
  const [predecessors, setPredecessors] = useState<string[]>([]);
  const [successors, setSuccessors] = useState<string[]>([]);
  const [historyBuildingId, setHistoryBuildingId] = useState("");
  const [history, setHistory] = useState<HouseNumberingBuildingLineage[]>([]);

  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function guarded(action: () => Promise<void>) {
    setError(null);
    try {
      await withCsrf(action);
    } catch (err) {
      setError(await uiError(err));
    }
  }

  async function transition(action: "submit" | "withdraw" | "approve" | "reject") {
    if (!proposal) return;
    await guarded(async () => {
      const updated = await transitionHouseNumberingProposal(
        hnClient,
        organizationId,
        projectId,
        proposal.id,
        {
          expected_revision: proposal.revision,
          action,
          comment: comment.trim() || null
        }
      );
      setProposal(updated);
      setQrOffset(0);
      await queue.reload();
      setNotice(`Phương án chuyển sang trạng thái: ${updated.status}.`);
    });
  }

  async function openProposal(id:string){
    setError(null);
    try{
      const result=await getHouseNumberingProposal(hnClient,organizationId,projectId,id);
      setProposal(result.proposal);setQrOffset(0);
      setNotice(`Đã mở lại phương án ${id.slice(0,8)} · ${result.items.length} hồ sơ.`);
    }catch(err){setError(await uiError(err));}
  }

  async function updatePlaque() {
    const building = buildings.data?.items.find(
      (item) => item.id === plaqueBuildingId
    );
    if (!building) return;
    await guarded(async () => {
      await setHouseNumberingPlaqueStatus(
        hnClient,
        organizationId,
        projectId,
        building.id,
        {
          expected_revision: building.revision,
          status: plaqueStatus
        }
      );
      setNotice(`Đã cập nhật trạng thái biển: ${plaqueStatus}.`);
      await buildings.reload();
    });
  }

  async function recordLineage() {
    const items = buildings.data?.items ?? [];
    const participantIds = [...new Set([...predecessors, ...successors])];
    const expectedRevisions = Object.fromEntries(
      participantIds.map((id) => {
        const building = items.find((item) => item.id === id);
        return [id, building?.revision ?? 0];
      })
    );
    await guarded(async () => {
      const result = await recordHouseNumberingBuildingLineage(
        hnClient,
        organizationId,
        projectId,
        {
          relation_kind: lineageKind,
          operation_id: crypto.randomUUID(),
          predecessor_building_ids: predecessors,
          successor_building_ids: successors,
          expected_revisions: expectedRevisions
        }
      );
      setNotice(
        `Đã ghi ${lineageKind} với ${result.items.length} cạnh lineage. Số cũ không tự chuyển sang công trình mới.`
      );
      await buildings.reload();
      const focus = successors[0] ?? predecessors[0];
      if (focus) {
        const rows = await listHouseNumberingBuildingLineage(
          hnClient,
          organizationId,
          projectId,
          focus
        );
        setHistoryBuildingId(focus);
        setHistory(rows.items);
      }
    });
  }

  async function loadHistory(buildingId: string) {
    setHistoryBuildingId(buildingId);
    if (!buildingId) {
      setHistory([]);
      return;
    }
    try {
      const rows = await listHouseNumberingBuildingLineage(
        hnClient,
        organizationId,
        projectId,
        buildingId
      );
      setHistory(rows.items);
    } catch (err) {
      setError(await uiError(err));
    }
  }

  function toggle(
    current: string[],
    setCurrent: (value: string[]) => void,
    id: string,
    checked: boolean
  ) {
    setCurrent(checked ? [...current, id] : current.filter((value) => value !== id));
  }

  return (
    <>
      <section className="hn-card">
        <h2>Hàng đợi trình duyệt số nhà</h2>
        <p className="hn-muted">Mở lại phương án do đồng nghiệp lập, xem tình trạng trình duyệt và tải hồ sơ. Phê duyệt yêu cầu quyền quản lý dự án và quy tắc có căn cứ.</p>
        <div className="hn-form">
          <label>Trạng thái<select aria-label="Lọc phương án" value={queueStatus} onChange={e=>{setQueueStatus(e.target.value as typeof queueStatus);setQueueOffset(0)}}>
            <option value="all">Tất cả</option><option value="draft">Dự thảo</option><option value="submitted">Chờ duyệt</option><option value="approved">Đã duyệt nội bộ</option>
          </select></label>
          <button className="hn-btn secondary" type="button" onClick={()=>void queue.reload()}>Tải lại hàng đợi</button>
        </div>
        {queue.error && <p className="hn-error">Không tải được hàng đợi: {String(queue.error)}</p>}
        <div style={{maxHeight:230,overflow:"auto"}}><table className="hn-table"><thead><tr><th>Phương án</th><th>Trạng thái</th><th>Hồ sơ</th><th>Quy tắc</th><th></th></tr></thead><tbody>{(queue.data?.items??[]).map(r=><tr key={r.id}>
          <td><code>{r.id.slice(0,8)}</code></td><td>{r.status}</td><td>{r.item_count}</td><td>{r.rule_version}</td>
          <td><button className="hn-btn secondary" type="button" onClick={()=>void openProposal(r.id)}>Mở để xử lý</button></td>
        </tr>)}</tbody></table></div>
        <div className="hn-actions"><button type="button" className="hn-btn secondary" disabled={queueOffset===0} onClick={()=>setQueueOffset(Math.max(0,queueOffset-100))}>Trước</button>
          <span>Từ {queueOffset+1}</span><button className="hn-btn secondary" type="button" disabled={queue.data?.next_offset==null} onClick={()=>setQueueOffset(queue.data?.next_offset??queueOffset)}>Sau</button></div>
      </section>
      <section className="hn-card">
        <h2>Phương án và phê duyệt</h2>
        <div className="hn-form">
          <label>
            Tuyến
            <select value={routeId} onChange={(e) => setRouteId(e.target.value)}>
              <option value="">Chọn tuyến</option>
              {(routes.data?.items ?? []).map((route) => (
                <option key={route.id} value={route.id}>
                  {route.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Rule
            <select value={ruleId} onChange={(e) => setRuleId(e.target.value)}>
              <option value="">Chọn rule</option>
              {(rules.data?.items ?? []).map((rule) => (
                <option key={rule.id} value={rule.id}>
                  {rule.version} · {rule.status}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="hn-checks">
          {(buildings.data?.items ?? [])
            .filter((building) => !routeId || building.route_id === routeId)
            .map((building) => (
              <label key={building.id}>
                <input
                  type="checkbox"
                  checked={selected.includes(building.id)}
                  onChange={(e) =>
                    setSelected((current) =>
                      e.target.checked
                        ? [...current, building.id]
                        : current.filter((id) => id !== building.id)
                    )
                  }
                />
                {building.parcel_ref ?? building.id.slice(0, 8)}
              </label>
            ))}
        </div>
        <div className="hn-form">
          <button
            className="hn-btn"
            type="button"
            disabled={!routeId || !ruleId || !selected.length}
            onClick={() =>
              void guarded(async () => {
                const result = await createHouseNumberingProposal(
                  hnClient,
                  organizationId,
                  projectId,
                  {
                    scope_kind: "route",
                    scope_id: routeId,
                    rule_id: ruleId,
                    building_ids: selected
                  }
                );
                setProposal(result.proposal);
                setQrOffset(0);
                await queue.reload();
                setNotice("Đã tạo phương án dự thảo. Có thể trình duyệt hoặc tải tài liệu đối soát.");
              })
            }
          >
            Tạo draft
          </button>
          <button
            className="hn-btn"
            type="button"
            disabled={!proposal || proposal.status !== "draft"}
            onClick={() => void transition("submit")}
          >
            Trình duyệt
          </button>
          <button
            className="hn-btn secondary"
            type="button"
            disabled={
              !proposal || proposal.status !== "submitted" || !comment.trim()
            }
            onClick={() => void transition("withdraw")}
          >
            Rút trình
          </button>
          <button
            className="hn-btn"
            type="button"
            disabled={!proposal || proposal.status !== "submitted"}
            onClick={() => void transition("approve")}
          >
            Phê duyệt
          </button>
          <button
            className="hn-btn danger"
            type="button"
            disabled={
              !proposal || proposal.status !== "submitted" || !comment.trim()
            }
            onClick={() => void transition("reject")}
          >
            Trả lại
          </button>
          <label>
            Ý kiến
            <textarea
              value={comment}
              onChange={(e) => setComment(e.target.value)}
              placeholder="Bắt buộc khi trả lại"
            />
          </label>
        </div>
        {proposal ? (
          <p>
            Proposal <code>{proposal.id}</code> ·{" "}
            <span className="hn-badge">{proposal.status}</span> · revision{" "}
            {proposal.revision}
          </p>
        ) : null}
        {proposal&&<div style={{borderTop:"1px solid #cbd5e1",paddingTop:12,marginTop:12}}>
          <h3>Hồ sơ xuất theo phương án</h3>
          <p className="hn-warning">Bản quyết định PDF luôn đóng dấu chữ “DỰ THẢO – CHƯA KÝ”, kể cả khi quy trình đã duyệt nội bộ. Chỉ cơ quan có thẩm quyền mới được ký và ban hành văn bản chính thức.</p>
          <div className="hn-actions">
            <a className="hn-btn secondary" href={houseNumberingProposalPacketUrl(organizationId,projectId,proposal.id,"decision")}>In dự thảo quyết định PDF</a>
            <a className="hn-btn secondary" href={houseNumberingProposalPacketUrl(organizationId,projectId,proposal.id,"register")}>Xuất danh sách số nhà Excel</a>
            <a className="hn-btn secondary" href={houseNumberingProposalPacketUrl(organizationId,projectId,proposal.id,"appendix",{offset:qrOffset,limit:24})}>In phụ lục PDF (24 hồ sơ)</a>
          </div>
          {proposal.status==="approved" ? <>
            <p className="hn-muted">Mẫu biển QR chỉ lấy số nhà đã duyệt và còn hiệu lực. QR trỏ tới bản đồ GIS nội bộ, yêu cầu tài khoản có quyền truy cập.</p>
            <div className="hn-form"><label>Trang in QR (24 biển/trang tải)<input aria-label="Lô QR bắt đầu từ" type="number" min={0} step={24} value={qrOffset} onChange={e=>setQrOffset(Math.max(0,Number(e.target.value)||0))}/></label>
              <a className="hn-btn secondary" href={houseNumberingProposalPacketUrl(organizationId,projectId,proposal.id,"plates",{offset:qrOffset,limit:24})}>Tải tem QR PDF</a>
            </div>
          </> : <p className="hn-warning">Chỉ mở chức năng tem QR sau khi phê duyệt phương án bằng quy tắc authoritative.</p>}
        </div>}
      </section>

      <section className="hn-card">
        <h2>Trạng thái gắn biển</h2>
        <div className="hn-form">
          <label>
            Công trình
            <select
              value={plaqueBuildingId}
              onChange={(e) => setPlaqueBuildingId(e.target.value)}
            >
              <option value="">Chọn công trình</option>
              {(buildings.data?.items ?? []).map((building) => (
                <option key={building.id} value={building.id}>
                  {building.parcel_ref ?? building.id.slice(0, 8)} ·{" "}
                  {building.plaque_status}
                </option>
              ))}
            </select>
          </label>
          <label>
            Trạng thái
            <select
              value={plaqueStatus}
              onChange={(e) =>
                setPlaqueStatus(
                  e.target.value as HouseNumberingBuilding["plaque_status"]
                )
              }
            >
              <option value="not_planned">Chưa kế hoạch</option>
              <option value="planned">Đã lên kế hoạch</option>
              <option value="installed">Đã gắn</option>
              <option value="replaced">Đã thay/cấp lại</option>
              <option value="removed">Đã tháo</option>
            </select>
          </label>
          <button
            className="hn-btn"
            type="button"
            disabled={!plaqueBuildingId}
            onClick={() => void updatePlaque()}
          >
            Cập nhật biển
          </button>
        </div>
        <p className="hn-muted">
          Gắn/thay biển chỉ được phép khi công trình có số approved.
        </p>
      </section>

      <section className="hn-card">
        <h2>Tách / gộp công trình</h2>
        <div className="hn-form">
          <label>
            Nghiệp vụ
            <select
              value={lineageKind}
              onChange={(e) => {
                setLineageKind(e.target.value as "split" | "merge");
                setPredecessors([]);
                setSuccessors([]);
              }}
            >
              <option value="split">Tách 1 → nhiều</option>
              <option value="merge">Gộp nhiều → 1</option>
            </select>
          </label>
        </div>
        <h3>Công trình nguồn</h3>
        <div className="hn-checks">
          {(buildings.data?.items ?? []).map((building) => (
            <label key={"pre-" + building.id}>
              <input
                type="checkbox"
                checked={predecessors.includes(building.id)}
                onChange={(e) =>
                  toggle(
                    predecessors,
                    setPredecessors,
                    building.id,
                    e.target.checked
                  )
                }
              />
              {building.parcel_ref ?? building.id.slice(0, 8)}
            </label>
          ))}
        </div>
        <h3>Công trình mới</h3>
        <div className="hn-checks">
          {(buildings.data?.items ?? []).map((building) => (
            <label key={"succ-" + building.id}>
              <input
                type="checkbox"
                checked={successors.includes(building.id)}
                onChange={(e) =>
                  toggle(successors, setSuccessors, building.id, e.target.checked)
                }
              />
              {building.parcel_ref ?? building.id.slice(0, 8)}
            </label>
          ))}
        </div>
        <button
          className="hn-btn"
          type="button"
          disabled={
            lineageKind === "split"
              ? predecessors.length !== 1 || successors.length < 2
              : predecessors.length < 2 || successors.length !== 1
          }
          onClick={() => void recordLineage()}
        >
          Ghi tách/gộp
        </button>
        <p className="hn-muted">
          Ledger chỉ ghi quan hệ và đóng công trình nguồn. Số cũ được giữ lịch sử;
          công trình mới phải qua preview/proposal riêng.
        </p>
      </section>

      <section className="hn-card">
        <h2>Lịch sử tách/gộp</h2>
        <label>
          Công trình
          <select
            value={historyBuildingId}
            onChange={(e) => void loadHistory(e.target.value)}
          >
            <option value="">Chọn công trình</option>
            {(buildings.data?.items ?? []).map((building) => (
              <option key={building.id} value={building.id}>
                {building.parcel_ref ?? building.id.slice(0, 8)}
              </option>
            ))}
          </select>
        </label>
        <table className="hn-table">
          <thead>
            <tr>
              <th>Loại</th>
              <th>Nguồn</th>
              <th>Đích</th>
              <th>Operation</th>
            </tr>
          </thead>
          <tbody>
            {history.map((item) => (
              <tr key={item.id}>
                <td>{item.relation_kind}</td>
                <td>{item.predecessor_building_id.slice(0, 8)}</td>
                <td>{item.successor_building_id.slice(0, 8)}</td>
                <td>
                  <code>{item.operation_id.slice(0, 8)}</code>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      {notice ? <div className="hn-ok">{notice}</div> : null}
      {error ? <div className="hn-error">{error}</div> : null}
    </>
  );
}
