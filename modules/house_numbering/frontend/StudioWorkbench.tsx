import {useState} from "react";
import {RegistryPanel} from "./Workspace";
import {NumberingPanel} from "./NumberingPanel";
import {ProposalPanel} from "./ProposalPanel";
import {ReconcilePanel} from "./ReconcilePanel";
import {SurveyPanel} from "./SurveyPanel";
import {DocumentsPanel} from "./DocumentsPanel";
import {SearchExportPanel} from "./SearchExportPanel";
import "./studio-workbench.css";

const tabs=[
 {id:"registry",label:"Địa bàn & công trình",detail:"Danh mục, đường/hẻm, nhà và liên kết GIS"},
 {id:"numbering",label:"Đánh số hàng loạt",detail:"Quy tắc, kiểm tra vị trí, xem trước và áp dụng"},
 {id:"proposal",label:"Phương án",detail:"Lập, trình và duyệt phương án"},
 {id:"reconcile",label:"Nhập & đối soát",detail:"Đối chiếu dữ liệu nguồn, xử lý sai khác"},
 {id:"survey",label:"Khảo sát & tiến độ",detail:"Giao việc, khảo sát và báo cáo tiến độ"},
 {id:"documents",label:"Hồ sơ & biển số",detail:"Hồ sơ, chứng nhận, in và lịch sử"},
 {id:"search",label:"Tra cứu & bàn giao",detail:"Tìm kiếm, xuất dữ liệu và bàn giao"}
] as const;
type Tab=typeof tabs[number]["id"];
type Props={organizationId:string;projectId:string;initialTab?:Tab;handoffSource?:string|undefined;handoffFeature?:string|undefined;onGoto:(source:string,feature:string)=>void};
export function StudioWorkbench({organizationId,projectId,initialTab="registry",handoffSource,handoffFeature,onGoto}:Props){
 const [tab,setTab]=useState<Tab>(initialTab);
 const current=tabs.find(t=>t.id===tab)!;
 const shared={organizationId,projectId};
 return <section className="hn-studio-workbench">
  <nav className="hn-studio-tabs" aria-label="Nghiệp vụ số nhà">{tabs.map(t=><button type="button" key={t.id} aria-pressed={tab===t.id} className={tab===t.id?"active":""} onClick={()=>setTab(t.id)}>{t.label}</button>)}</nav>
  <div className="hn-studio-content"><div className="hn-studio-lead"><h3>{current.label}</h3><p>{current.detail}</p></div>
  {tab==="registry"&&<RegistryPanel {...shared} handoffSource={handoffSource} handoffFeature={handoffFeature} onGoto={onGoto}/>}
  {tab==="numbering"&&<NumberingPanel {...shared}/>}
  {tab==="proposal"&&<ProposalPanel {...shared}/>}
  {tab==="reconcile"&&<ReconcilePanel {...shared}/>}
  {tab==="survey"&&<SurveyPanel {...shared}/>}
  {tab==="documents"&&<DocumentsPanel {...shared}/>}
  {tab==="search"&&<SearchExportPanel {...shared}/>}
  </div>
 </section>;
}
