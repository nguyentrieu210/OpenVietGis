import React, {lazy, Suspense} from "react";
import {createRoot} from "react-dom/client";
import {entryRoute} from "./entry-route";
import "./style.css";

const Workspace = lazy(() => import("./SessionEntry").then(m => ({default:m.LiveApp})));
const Offline = lazy(() => import("./offline-entry"));
const Auth = lazy(() => import("./Landing").then(m => ({default: m.AuthComplete})));
const Marketplace = lazy(() => import("./MarketplacePage").then(m => ({default: m.MarketplacePage})));
const Qgis = lazy(() => import("./QgisConnectPage").then(m => ({default: m.QgisConnectPage})));
const Publisher = lazy(() => import("./PublisherPortal").then(m => ({default: m.PublisherPortal})));
const routes = {workspace: Workspace, offline: Offline, auth: Auth, marketplace: Marketplace, qgis: Qgis, developer: Publisher};
const route = entryRoute(location.pathname, location.search);
const Page = routes[route];
if (import.meta.env.PROD && import.meta.env.BASE_URL === "/" && "serviceWorker" in navigator && window.isSecureContext) {
  const register = () => void navigator.serviceWorker.register("/offline-shell-sw.js").catch(error => console.warn("Offline shell unavailable", error));
  // Offline users explicitly need readiness; other routes should not contend with
  // their critical entry request. Registration remains automatic after page load.
  if (route === "offline" || document.readyState === "complete") register();
  else window.addEventListener("load", register, {once:true});
}
createRoot(document.getElementById("root")!).render(
  <Suspense fallback={<p role="status" style={{padding:24}}>Đang mở OpenVietGIS…</p>}><Page/></Suspense>
);
