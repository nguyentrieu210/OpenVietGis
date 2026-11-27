import {BrowserRouter, Route, Routes} from "react-router-dom";
import {OfflineFieldPage} from "./workbenches/features/offline/OfflineFieldPage";
import "./workbenches/ui/tokens.css";
import "./workbenches/features/platform/platform.css";
export default function OfflineEntry() {
  return <BrowserRouter basename={location.pathname.startsWith("/devui/") ? "/devui" : undefined}>
    <Routes><Route path="/offline-field/:organizationId/:projectId" element={<OfflineFieldPage/>}/></Routes>
  </BrowserRouter>;
}
