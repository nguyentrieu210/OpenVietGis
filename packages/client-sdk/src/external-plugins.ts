import {parsePluginV2Runtime,buildPluginV2Invocation} from "./plugin-v2-contract";
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
 license_required?:boolean;
  envelope: {
    schema_version: 2; publisher_id: string; extension_key: string; version: string;
    manifest_digest: string; host_api_range: string; platforms: string[];
    permissions: string[]; contribution_digest: string;
    assets: {id:string;media_type:string;size:number;sha256:string}[];
    dependencies?: {key:string;range:string}[];
    issued_at: string; key_id: string;
  };
  release_digest: string; catalog_epoch: number; runtime_available: false;
  publisher_signature: string; operator_signature: string;
};
export function listMarketplaceV2(client: ApiClient, host: "web"|"android"|"ios"|"windows"|"macos"|"linux"|"server", api: string) {
  return client.request<{items:MarketplaceV2Metadata[];profile:"metadata_only";web_read_only_enabled?:boolean;host_availability?:{platform:string;profile:"web_read_only"|"metadata_only";online_execution:boolean;offline_execution:false;reason:string}}>("/marketplace/v2/catalog?" + new URLSearchParams({host,api}).toString(), {cache:"no-store"});
}

export type PluginV2Field = {kind:"text"|"number"|"integer"|"boolean";required:boolean};
export type PluginV2Widget = {kind:"text";id:string;text:string} | {kind:"input";id:string;label:string;field:string} | {kind:"button";id:string;label:string;action:string} | {kind:"group";id:string;children:PluginV2Widget[]};
export type PluginV2Document = {slot:"workspace"|"map_tool"|"form"|"dashboard";widgets:PluginV2Widget[];actions:{id:string;command_id:string;permission:string;consent:"read";timeout_ms:number;input:Record<string,PluginV2Field>}[]};
export type PluginV2Runtime = {document:PluginV2Document;release_digest:string;activation_revision:number;manifest_id:string;profile:"web_read_only";granted:boolean;grant_revision:string;permissions:string[];lock_digest:string;catalog_epoch:number;dependency_lock:{key:string;version:string;release_digest:string;manifest_id:string;activation_id:string;activation_revision:number}[]};
const pluginV2Path=(org:string,project:string,manifest:string)=>"/organizations/"+encodeURIComponent(org)+"/projects/"+encodeURIComponent(project)+"/extensions/"+encodeURIComponent(manifest)+"/v2-runtime";
export function getPluginV2Runtime(client:ApiClient,org:string,project:string,manifest:string,signal?:AbortSignal){
 return client.request<unknown>(pluginV2Path(org,project,manifest),{signal:signal??null,cache:"no-store"}).then(r=>parsePluginV2Runtime(r,manifest));
}
export function invokePluginV2(client:ApiClient,org:string,project:string,runtime:PluginV2Runtime,actionId:string,input:Record<string,unknown>,requestId:string,signal?:AbortSignal){
 return client.request<{receipt_id:string;command_id:string;output:unknown}>(pluginV2Path(org,project,runtime.manifest_id)+"/invoke",{method:"POST",signal:signal??null,body:JSON.stringify(buildPluginV2Invocation(runtime,actionId,input,requestId))});
}

export function setPluginV2Grant(client:ApiClient,org:string,project:string,runtime:PluginV2Runtime,enabled:boolean,signal?:AbortSignal){
 return client.request<{granted:boolean;grant_revision:string}>(pluginV2Path(org,project,runtime.manifest_id)+"/grant",{method:"POST",signal:signal??null,body:JSON.stringify({release_digest:runtime.release_digest,activation_revision:runtime.activation_revision,grant_revision:runtime.grant_revision,lock_digest:runtime.lock_digest,enabled})});
}

export type PluginSandboxQuote={order_id:string;organization_id:string;extension_key:string;currency:string;gross_minor:number;tax_minor:number;publisher_bps:number;access_until:string};
export type PluginSandboxEvent={provider_event_id:string;order_id:string;organization_id:string;kind:"capture"|"refund"|"chargeback";currency:string;amount_minor:number;occurred_at:string};
const sandboxOrderPath=(org:string)=>"/organizations/"+encodeURIComponent(org)+"/marketplace/sandbox/orders";
export function createPluginSandboxOrder(client:ApiClient,quote:PluginSandboxQuote,releaseDigest:string){
 return client.request<{sandbox:true;quote:PluginSandboxQuote;release_digest:string}>(sandboxOrderPath(quote.organization_id),{method:"POST",body:JSON.stringify({quote,release_digest:releaseDigest})});
}
export function getPluginSandboxOrder(client:ApiClient,org:string,id:string){
 return client.request<{sandbox:true;order_id:string;release_digest:string;entitlement:"pending"|"active"|"expired"|"revoked";outstanding_minor:number;event_count:number;receipts:unknown[];checkout_offer_digest?:string|null;checkout_expires_at?:string|null}>(sandboxOrderPath(org)+"/"+encodeURIComponent(id),{cache:"no-store"});
}
export function submitPluginSandboxEvent(client:ApiClient,event:PluginSandboxEvent,signature:string){
 return client.request<{sandbox:true;replayed:boolean;receipt:unknown;entitlement:"pending"|"active"|"expired"|"revoked";outstanding_minor:number}>(sandboxOrderPath(event.organization_id)+"/"+encodeURIComponent(event.order_id)+"/events",{method:"POST",body:JSON.stringify({event,signature})});
}

export type PluginGraphSelection={root:string;version:string;rollback_of?:string};
export type PluginActivationPin={key:string;manifest_id:string;activation_id:string;revision:number;status:"active"|"disabled"};
export type PluginGraphPreview={plan:{catalog_epoch:number;pins:{key:string;version:string;release_digest:string;permissions:string[]}[];expected:PluginActivationPin[]|null;rollback_of:string|null};plan_digest:string;grants_created:false};
const graphPath=(org:string,project?:string)=>"/organizations/"+encodeURIComponent(org)+(project?"/projects/"+encodeURIComponent(project)+"/extensions/v2-graph":"/marketplace/v2/graph");
export function previewPluginGraph(client:ApiClient,org:string,selection:PluginGraphSelection,project?:string){
 return client.request<PluginGraphPreview>(graphPath(org,project)+"/preview",{method:"POST",body:JSON.stringify(selection)});
}
export function installPluginGraph(client:ApiClient,org:string,selection:PluginGraphSelection,preview:PluginGraphPreview,requestId:string){
 return client.request<{request_id:string;plan_digest:string;items:{manifest_id:string;key:string;version:string;release_digest:string;reused:boolean}[];activated:false}>(graphPath(org)+"/install",{method:"POST",body:JSON.stringify({selection,request_id:requestId,plan_digest:preview.plan_digest})});
}
export function applyPluginGraph(client:ApiClient,org:string,project:string,selection:PluginGraphSelection,preview:PluginGraphPreview,requestId:string){
 if(!preview.plan.expected)throw new Error("PROJECT_PREVIEW_REQUIRED");
 return client.request<{request_id:string;plan_digest:string;before:PluginActivationPin[];after:PluginActivationPin[];rollback_of:string|null;grants_created:false}>(graphPath(org,project)+"/apply",{method:"POST",body:JSON.stringify({selection,request_id:requestId,plan_digest:preview.plan_digest,expected:preview.plan.expected})});
}

export type PluginProcessingProposal={proposal_id:string;plan_digest:string;plan:unknown;requires_human_confirmation:true};
export function confirmPluginProcessing(client:ApiClient,org:string,project:string,runtime:PluginV2Runtime,proposal:PluginProcessingProposal,signal?:AbortSignal){
 return client.request<{proposal_id:string;job:{id:string;status:string};execution:"manual_core_job"}>(pluginV2Path(org,project,runtime.manifest_id)+"/processing/confirm",{method:"POST",signal:signal??null,body:JSON.stringify({proposal_id:proposal.proposal_id,plan_digest:proposal.plan_digest})});
}


export type PluginLicenseState="pending"|"trial"|"active"|"grace"|"expired"|"revoked"|"missing";
const licensePath=(org:string)=>"/organizations/"+encodeURIComponent(org)+"/marketplace/v2/licenses";
export function importPluginLicense(client:ApiClient,org:string,body:unknown){
 return client.request<{grant_id:string;sequence:number;state:PluginLicenseState;replayed:boolean}>(licensePath(org),{method:"POST",body:JSON.stringify(body)});
}
export function getPluginLicense(client:ApiClient,org:string,digest:string){
 return client.request<{release_digest:string;state:PluginLicenseState;allowed:boolean}>(licensePath(org)+"/"+encodeURIComponent(digest),{cache:"no-store"});
}

export type SandboxSettlement={currency:string;event_count:number;event_digest:string;clearing_minor:number;publisher_payable_minor:number;platform_revenue_minor:number;tax_payable_minor:number};
export function getSandboxSettlement(client:ApiClient,org:string,order:string){
 return client.request<{sandbox:true;report:SandboxSettlement}>(sandboxOrderPath(org)+"/"+encodeURIComponent(order)+"/settlement",{cache:"no-store"});
}
export function reconcileSandboxSettlement(client:ApiClient,org:string,order:string,statement:SandboxSettlement){
 return client.request<{sandbox:true;statement_verified:false;report:SandboxSettlement;comparison:{matched:boolean;mismatches:string[]}}>(sandboxOrderPath(org)+"/"+encodeURIComponent(order)+"/reconcile",{method:"POST",body:JSON.stringify(statement)});
}

export type SandboxSignedStatement={schema:"openvietgis.settlement.sandbox.statement.v1";organization_id:string;order_id:string;observed_at:string;report:SandboxSettlement};
export function reconcileSignedSandboxSettlement(client:ApiClient,statement:SandboxSignedStatement,signature:string){
 return client.request<{sandbox:true;statement_verified:true;report:SandboxSettlement;comparison:{matched:boolean;mismatches:string[]}}>(sandboxOrderPath(statement.organization_id)+"/"+encodeURIComponent(statement.order_id)+"/reconcile-signed",{method:"POST",body:JSON.stringify({statement,signature})});
}

export type SignedSandboxPriceOffer={schema:"openvietgis.checkout.sandbox.offer.v1";audience:string;release_digest:string;publisher_id:string;quote:PluginSandboxQuote;issued_at:string;expires_at:string};
export function previewSandboxPriceOffer(client:ApiClient,offer:SignedSandboxPriceOffer,signature:string){
 return client.request<{sandbox:true;offer:SignedSandboxPriceOffer;offer_digest:string;checkout_created:false}>("/organizations/"+encodeURIComponent(offer.quote.organization_id)+"/marketplace/sandbox/offers/preview",{method:"POST",body:JSON.stringify({offer,signature})});
}

export function acceptSandboxPriceOffer(client:ApiClient,offer:SignedSandboxPriceOffer,signature:string){
 return client.request<{sandbox:true;checkout_created:true;provider_session_created:false;replayed:boolean;order_id:string;offer_digest:string;expires_at:string}>("/organizations/"+encodeURIComponent(offer.quote.organization_id)+"/marketplace/sandbox/offers/accept",{method:"POST",body:JSON.stringify({offer,signature})});
}


export type PluginAuditReceipt={id:string;request_id:string;activation_id:string;activation_revision:string;manifest_id:string;release_digest:string;request_digest:string;command_id:string;created_at:string};
export function listOwnPluginAudit(client:ApiClient,org:string,project:string,afterId?:string,limit=50,signal?:AbortSignal){
 const q=new URLSearchParams({limit:String(limit)});if(afterId)q.set("after_id",afterId);
 return client.request<{items:PluginAuditReceipt[];next_after_id:string|null;limit:number;scope:"own_project_receipts"}>("/organizations/"+encodeURIComponent(org)+"/projects/"+encodeURIComponent(project)+"/extensions/v2-audit?"+q.toString(),{cache:"no-store",signal:signal??null});
}
