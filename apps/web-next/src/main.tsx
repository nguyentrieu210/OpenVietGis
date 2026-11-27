import React,{Suspense} from "react";
import {BrowserRouter,Route,Routes} from "react-router-dom";
import {Field} from "./workbench-registry";
import {createRoot} from "react-dom/client";
import {configureMapSurface} from "./map-surface";
import "./style.css";
import {LiveApp} from "./LiveApp";
import {AuthComplete} from "./Landing";
import {MarketplacePage} from "./MarketplacePage";
import {QgisConnectPage} from "./QgisConnectPage";
import {PublisherPortal} from "./PublisherPortal";
configureMapSurface();
const params=new URLSearchParams(location.search);
const base=import.meta.env.BASE_URL;
const offlineRoute=/^\/offline-field\/[^/]+\/[^/]+\/?$/.test(location.pathname.replace(/^\/devui(?=\/)/,""));
if(import.meta.env.PROD&&base==="/"&&"serviceWorker" in navigator&&window.isSecureContext){
 void navigator.serviceWorker.register("/offline-shell-sw.js").catch(error=>{console.warn("Offline shell unavailable",error);});
}
const offlinePage=<BrowserRouter basename={location.pathname.startsWith("/devui/")?"/devui":undefined}><Suspense fallback={<p role="status">Đang mở gói thực địa…</p>}><Routes><Route path="/offline-field/:organizationId/:projectId" element={<Field/>}/></Routes></Suspense></BrowserRouter>;

createRoot(document.getElementById("root")!).render(offlineRoute?offlinePage:params.get("qgis_connect")==="1"?<QgisConnectPage/>:params.get("auth_complete")==="1"?<AuthComplete/>:params.get("marketplace")==="1"?<MarketplacePage/>:params.get("developer")==="1"?<PublisherPortal/>:<LiveApp/>);
