import type { ApiClient } from "./http";
import type { ExtensionManifest } from "./platform";

export const PARTNER_CAPABILITIES = ["host_context", "host_notice", "map_view", "map_navigate", "map_selection", "map_overlay", "tool_register"] as const;
export type PartnerCapability = typeof PARTNER_CAPABILITIES[number];
export type PartnerPluginInput = {
  extension_key: string;
  version: string;
  display_name: string;
  description: string;
  url: string;
  capabilities: PartnerCapability[];
};

export function safePartnerUrl(value: unknown): string | null {
  if (typeof value !== "string" || value.length > 2048 || /[^\x21-\x7e]|[\\?#]/.test(value)) return null;
  try {
    const url = new URL(value);
    const host = url.hostname;
    if (url.protocol !== "https:" || !/^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}$/.test(host)
      || !value.startsWith("https://" + host + "/") || url.username || url.password || url.port
      || host === "openvietgis.cloud" || /\.(openvietgis\.cloud|local|localhost|internal|test)$/.test(host)
      || (typeof location !== "undefined" && host === location.hostname)) return null;
    return url.href;
  } catch { return null; }
}

export function partnerPanel(manifest: ExtensionManifest): {url: string; capabilities: PartnerCapability[]} | null {
  if (!manifest.enabled || !/^partner_[a-z0-9_-]+$/.test(manifest.extension_key) || manifest.api_compatibility !== "1.x"
    || !Array.isArray(manifest.permissions) || manifest.permissions.length || !Array.isArray(manifest.commands) || manifest.commands.length
    || !manifest.entity_schemas || typeof manifest.entity_schemas !== "object" || Array.isArray(manifest.entity_schemas) || Object.keys(manifest.entity_schemas).length
    || !Array.isArray(manifest.capabilities) || !manifest.capabilities.length || new Set(manifest.capabilities).size !== manifest.capabilities.length
    || !manifest.capabilities.every(c => (PARTNER_CAPABILITIES as readonly string[]).includes(c))
    || !Array.isArray(manifest.ui_contributions) || manifest.ui_contributions.length !== 1) return null;
  const panel = manifest.ui_contributions[0];
  if (!panel || Object.keys(panel).length !== 7 || panel.id !== "partner_panel" || panel.slot !== "project_panel"
    || panel.route_suffix !== "/partner" || panel.runtime !== "sandboxed_iframe" || panel.sdk_version !== "1.0"
    || typeof panel.label !== "string" || !panel.label.trim()) return null;
  const url = safePartnerUrl(panel.url);
  return url ? {url, capabilities: manifest.capabilities as PartnerCapability[]} : null;
}

export function registerPartnerPlugin(client: ApiClient, organizationId: string, input: PartnerPluginInput, csrf: string) {
  const {url, ...metadata} = input;
  if (!safePartnerUrl(url)) throw Error("URL plugin phải là HTTPS ở tên miền đối tác, không có query hoặc fragment.");
  return client.request<ExtensionManifest>(`/organizations/${encodeURIComponent(organizationId)}/extensions/manifests`, {
    method: "POST", headers: {"x-openvietgis-csrf": csrf},
    body: JSON.stringify({...metadata, api_compatibility: "1.x", permissions: [], commands: [], entity_schemas: {},
      ui_contributions: [{id: "partner_panel", slot: "project_panel", label: input.display_name,
        route_suffix: "/partner", runtime: "sandboxed_iframe", url, sdk_version: "1.0"}]})
  });
}

export type PartnerContributionDocument={overlay:unknown;tools:unknown[]};
export type SavedPartnerContribution={manifest_id:string;display_name:string;document:PartnerContributionDocument;revision:number;updated_at:string};
export function listPartnerContributions(client:ApiClient,org:string,project:string){
 return client.request<{items:SavedPartnerContribution[]}>(`/organizations/${encodeURIComponent(org)}/projects/${encodeURIComponent(project)}/extensions/partner-contributions`);
}
export function savePartnerContributions(client:ApiClient,org:string,project:string,manifestId:string,expectedRevision:number,document:PartnerContributionDocument,csrf:string){
 return client.request<SavedPartnerContribution>(`/organizations/${encodeURIComponent(org)}/projects/${encodeURIComponent(project)}/extensions/partner-contributions/${encodeURIComponent(manifestId)}`,{method:"PUT",headers:{"x-openvietgis-csrf":csrf},body:JSON.stringify({expected_revision:expectedRevision,document})});
}
export type MarketplaceRelease={publisher_id:string;publisher_name:string;signature:string;manifest:Omit<ExtensionManifest,"id"|"enabled">};
export function listMarketplace(client:ApiClient){return client.request<{items:MarketplaceRelease[]}>("/marketplace/catalog")}
export function installMarketplaceRelease(client:ApiClient,org:string,release:MarketplaceRelease,csrf:string){
 const m=release.manifest;
 return client.request<ExtensionManifest>(`/organizations/${encodeURIComponent(org)}/marketplace/install/${encodeURIComponent(release.publisher_id)}/${encodeURIComponent(m.extension_key)}/${encodeURIComponent(m.version)}`,{method:"POST",headers:{"x-openvietgis-csrf":csrf}});
}


/** Paginated search of the same reviewed, signed v1 catalog. */
export function searchMarketplace(client: ApiClient, query: string, offset = 0, limit = 50) {
  if ([...query].length > 200 || !Number.isInteger(offset) || offset < 0 || offset > 100000 || !Number.isInteger(limit) || limit < 1 || limit > 100) throw new Error("Invalid marketplace search");
  const params = new URLSearchParams({q: query, offset: String(offset), limit: String(limit)});
  return client.request<{items: MarketplaceRelease[]; total: number; next_offset: number | null}>("/marketplace/search?" + params.toString(), {cache:"no-store"});
}


/** V2 metadata is cryptographically verified by the configured server.
 * This is not an installed or executable plugin. */
export type MarketplaceV2Metadata = {
  envelope: {
    schema_version: 2; publisher_id: string; extension_key: string; version: string;
    manifest_digest: string; host_api_range: string; platforms: string[];
    permissions: string[]; contribution_digest: string;
    assets: {id:string;media_type:string;size:number;sha256:string}[];
    issued_at: string; key_id: string;
  };
  release_digest: string; catalog_epoch: number; runtime_available: false;
  publisher_signature: string; operator_signature: string;
};
export function listMarketplaceV2(client: ApiClient, host: "web"|"android"|"ios"|"windows"|"macos"|"linux"|"server", api: string) {
  return client.request<{items:MarketplaceV2Metadata[];profile:"metadata_only"}>("/marketplace/v2/catalog?" + new URLSearchParams({host,api}).toString(), {cache:"no-store"});
}
