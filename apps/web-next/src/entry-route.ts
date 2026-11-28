// Preserve entry precedence independently of the heavyweight application graph.
export type EntryRoute = "offline" | "qgis" | "auth" | "marketplace" | "developer" | "workspace";
export function entryRoute(pathname: string, search: string): EntryRoute {
  const path = pathname.replace(/^\/devui(?=\/)/, "");
  if (/^\/offline-field\/[^/]+\/[^/]+\/?$/.test(path)) return "offline";
  const params = new URLSearchParams(search);
  if (params.get("qgis_connect") === "1") return "qgis";
  if (params.get("auth_complete") === "1") return "auth";
  if (params.get("marketplace") === "1") return "marketplace";
  if (params.get("developer") === "1") return "developer";
  return "workspace";
}
