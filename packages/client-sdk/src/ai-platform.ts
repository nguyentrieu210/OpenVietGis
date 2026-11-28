/** AI platform v1: use only in an authenticated hosted workspace.
 * Never import a browser bridge token into a native local workspace.
 */
export const AI_PLATFORM_VERSION = 1;
export const AI_CLOUD_URL = "https://openvietgis.cloud/";
export type AiAction = "providers" | "run" | "acp" | "status" | "cancel";
export interface AiWorkspaceSession { session_id: string; browser_token: string }
export interface AiProvider { id: string; model: string; local: boolean; vision: boolean }
export interface AiCapabilities {
  version: number; capabilities: Record<string, unknown>;
}
export interface AiDiscovery { providers: AiProvider[]; acp_agents: string[]; platform: AiCapabilities }
export interface AiSnapshot { data: string; width: number; height: number }
export interface AiRunInput {
  provider: string; expected_model: string; user: string; skill: string | null;
  consent: true; allow_writes: boolean; multi_agent: boolean;
  snapshot?: AiSnapshot; vision_consent?: true;
}
export interface AiAcpInput { agent: string; task: string; consent: true }
export interface AiRunStarted { run_id: string; state: "running" }
export interface AiReport {
  stop: string; answer?: string | null; calls?: number | null; model_steps?: number | null;
  uncertain_call?: string | null; unresolved_ticket?: string | null;
  children?: unknown[]; context?: Record<string, unknown>; acp?: boolean;
}
export type AiStatus = {state:"running"} | {state:"done";report:AiReport};
const actions = new Set<AiAction>(["providers","run","acp","status","cancel"]);
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
/** Separate from ApiClient: no CSRF refresh or automatic retry of AI operations. */
export async function requestAi<T>(s: AiWorkspaceSession, action: AiAction, payload: object, signal?: AbortSignal, fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis)): Promise<T> {
  if (!uuid.test(s.session_id) || !s.browser_token || s.browser_token.length>1024 || !actions.has(action)) throw new TypeError("Invalid AI workspace request");
  const body=JSON.stringify(payload);
  if(new TextEncoder().encode(body).length>128000)throw new TypeError("AI request too large");
  const response=await fetchImpl("/ai-copilot/agent/"+s.session_id+"/"+action,{
    method:"POST",headers:{"X-OVG-AI":"1","X-OVG-Bridge":s.browser_token,"Content-Type":"application/json"},
    credentials:"same-origin",redirect:"error",cache:"no-store",body,...(signal?{signal}:{})
  });
  if(!response.ok)throw new Error("AI Agent không khả dụng ("+response.status+")");
  const value:unknown=await response.json();
  if(!value||typeof value!=="object"||Array.isArray(value))throw new Error("Invalid AI response");
  if(action==="providers"&&(!("platform" in value)||!(value.platform&&typeof value.platform==="object"&&"version" in value.platform&&value.platform.version===AI_PLATFORM_VERSION)))throw new Error("Unsupported AI platform version");
  return value as T;
}
export class AiClient {
  constructor(private readonly session:AiWorkspaceSession,private readonly transport:typeof fetch=globalThis.fetch.bind(globalThis)){}
  discover(signal?:AbortSignal){return requestAi<AiDiscovery>(this.session,"providers",{},signal,this.transport)}
  run(input:AiRunInput,signal?:AbortSignal){return requestAi<AiRunStarted>(this.session,"run",input,signal,this.transport)}
  acp(input:AiAcpInput,signal?:AbortSignal){return requestAi<AiRunStarted>(this.session,"acp",input,signal,this.transport)}
  status(run_id:string,signal?:AbortSignal){return requestAi<AiStatus>(this.session,"status",{run_id},signal,this.transport)}
  cancel(run_id:string,signal?:AbortSignal){return requestAi<{state:"cancel_requested"}>(this.session,"cancel",{run_id},signal,this.transport)}
}
