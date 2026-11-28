//! Native HTTP/OAuth candidate. The legacy production service remains in place
//! until real consumer and permission-parity gates pass.
use axum::{Router, extract::{State,Path,DefaultBodyLimit}, body::Bytes, http::{HeaderMap,StatusCode,header}, response::{IntoResponse,Response}, routing::{get,post}, Json};
use jsonwebtoken::{decode,decode_header,Algorithm,DecodingKey,Validation,jwk::JwkSet};
use openvietgis_mcp_rust::{Store,Owner,Error,CommandState,tools::{Registry,dispatch}};
use serde::{Deserialize,Deserializer,de::{self,Visitor,MapAccess,SeqAccess}};
use serde_json::{Value,json,Map};
use std::{sync::{Arc,Mutex,MutexGuard},time::{Instant,SystemTime,UNIX_EPOCH,Duration},collections::HashSet,fmt};
use uuid::Uuid;
const ORIGIN:&str="https://openvietgis.cloud";
const ISSUER:&str="https://openvietgis.cloud/realms/openvietgis";
const RESOURCE:&str="https://openvietgis.cloud/mcp";
const CORE:&str="http://127.0.0.1:8019/api/v1";
const JWKS:&str="http://127.0.0.1:18080/realms/openvietgis/protocol/openid-connect/certs";
const CLIENTS:[&str;2]=["openvietgis-mcp-chatgpt","openvietgis-mcp-claude"];
const PROTOCOLS:[&str;4]=["2025-03-26","2025-06-18","2025-11-25","2026-07-28"];
#[derive(Clone)]
struct App {store:Arc<Mutex<Store>>,registry:Arc<Registry>,http:reqwest::Client,clock:Instant,jwks:Arc<tokio::sync::Mutex<Option<(Instant,JwkSet)>>>,core:String,jwks_url:String}
impl App{
    fn now(&self)->u64{self.clock.elapsed().as_millis().try_into().unwrap_or(u64::MAX)}
    fn store(&self)->Result<MutexGuard<'_,Store>,Failure>{self.store.lock().map_err(|_|Failure::new(503,"Workspace store unavailable"))}
}
#[derive(Debug)]
struct Failure{status:u16,msg:&'static str}
impl Failure{fn new(status:u16,msg:&'static str)->Self{Self{status,msg}}}
impl From<Error> for Failure{
    fn from(e:Error)->Self{match e{
        Error::Unavailable=>Self::new(404,"Workspace not connected or unavailable"),
        Error::AmbiguousWorkspace=>Self::new(409,"Multiple workspaces connected; call gis_workspaces and supply session_id"),
        Error::PreviousUnread=>Self::new(409,"Previous command still pending or unread"),
        Error::RateLimited=>Self::new(429,"Try again after two seconds"),
        Error::Capacity=>Self::new(429,"Workspace capacity reached"),
        Error::Invalid=>Self::new(422,"Invalid or oversized command/result"),
        Error::CommandUnavailable=>Self::new(409,"Command unavailable"),
        Error::TokenExpired=>Self::new(401,"OAuth token expired"),
        Error::Version=>Self::new(409,"Update the workspace to use this tool"),
        Error::ModeMismatch=>Self::new(409,"Tool not available in the active workspace mode; call gis_workspaces"),
    }}
}
impl IntoResponse for Failure{
    fn into_response(self)->Response{respond(self.status,json!({"error":self.msg}))}
}
fn respond(status:u16,v:Value)->Response{
    let mut response=(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),Json(v)).into_response();
    response.headers_mut().insert(header::CACHE_CONTROL,header::HeaderValue::from_static("no-store"));response
}
fn hdr<'a>(h:&'a HeaderMap,k:&str)->&'a str{h.get(k).and_then(|x|x.to_str().ok()).unwrap_or("")}
fn uuid(s:&Value)->Result<Uuid,Failure>{s.as_str().and_then(|s|Uuid::parse_str(s).ok()).ok_or(Failure::new(422,"Invalid identifier"))}
fn origin_ok(h:&HeaderMap)->bool{!h.contains_key(header::ORIGIN)||hdr(h,"origin")==ORIGIN}
fn trusted_host(h:&HeaderMap)->bool{hdr(h,"host")=="openvietgis.cloud"}
fn unix_now()->u64{SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()}
fn unauthorized()->Response{
    let mut r=respond(401,json!({"error":"invalid_token","error_description":"Authentication required"}));
    r.headers_mut().insert(header::WWW_AUTHENTICATE,header::HeaderValue::from_static("Bearer resource_metadata=\"https://openvietgis.cloud/.well-known/oauth-protected-resource/mcp\""));
    r
}
// Deserialize strict JSON and reject repeated members rather than accepting
// last-member wins semantics at the authorization/command boundary.
struct Strict(Value);
impl<'de> Deserialize<'de> for Strict{
    fn deserialize<D:Deserializer<'de>>(d:D)->Result<Self,D::Error>{
        struct V;
        impl<'de> Visitor<'de> for V{
            type Value=Strict;
            fn expecting(&self,f:&mut fmt::Formatter)->fmt::Result{write!(f,"strict JSON value")}
            fn visit_bool<E:de::Error>(self,v:bool)->Result<Strict,E>{Ok(Strict(json!(v)))}
            fn visit_i64<E:de::Error>(self,v:i64)->Result<Strict,E>{Ok(Strict(json!(v)))}
            fn visit_u64<E:de::Error>(self,v:u64)->Result<Strict,E>{Ok(Strict(json!(v)))}
            fn visit_f64<E:de::Error>(self,v:f64)->Result<Strict,E>{if v.is_finite(){Ok(Strict(json!(v)))}else{Err(E::custom("non-finite"))}}
            fn visit_str<E:de::Error>(self,v:&str)->Result<Strict,E>{Ok(Strict(json!(v)))}
            fn visit_string<E:de::Error>(self,v:String)->Result<Strict,E>{Ok(Strict(json!(v)))}
            fn visit_unit<E:de::Error>(self)->Result<Strict,E>{Ok(Strict(Value::Null))}
            fn visit_none<E:de::Error>(self)->Result<Strict,E>{Ok(Strict(Value::Null))}
            fn visit_seq<A:SeqAccess<'de>>(self,mut a:A)->Result<Strict,A::Error>{let mut out=Vec::new();while let Some(v)=a.next_element::<Strict>()?{out.push(v.0);}Ok(Strict(Value::Array(out)))}
            fn visit_map<A:MapAccess<'de>>(self,mut a:A)->Result<Strict,A::Error>{let mut out=Map::new();while let Some((k,v))=a.next_entry::<String,Strict>()?{if out.insert(k,v.0).is_some(){return Err(de::Error::custom("duplicate member"))}}Ok(Strict(Value::Object(out)))}
        }
        d.deserialize_any(V)
    }
}
fn parse(body:&Bytes,h:&HeaderMap)->Result<Value,Failure>{
    if hdr(h,"content-type").split(';').next()!=Some("application/json"){return Err(Failure::new(415,"JSON required"))}
    if body.len()>128_000{return Err(Failure::new(413,"Request too large"))}
    let v=serde_json::from_slice::<Strict>(body).map_err(|_|Failure::new(422,"Invalid JSON"))?.0;
    if !v.is_object(){return Err(Failure::new(422,"JSON object required"))}
    let mut stack=vec![(&v,0)];while let Some((v,d))=stack.pop(){if d>64{return Err(Failure::new(422,"JSON nesting too deep"))}match v{Value::Array(a)=>stack.extend(a.iter().map(|v|(v,d+1))),Value::Object(m)=>stack.extend(m.values().map(|v|(v,d+1))),_=>{}}}
    Ok(v)
}
#[derive(Deserialize)]
struct Claims{iss:String,sub:String,azp:String,scope:String,typ:String,exp:u64,iat:u64}
async fn verify(app:&App,h:&HeaderMap)->Result<Claims,Failure>{
    let err=||Failure::new(401,"Invalid OAuth token");
    let token=hdr(h,"authorization").strip_prefix("Bearer ").filter(|v|!v.is_empty()&&v.len()<=12000).ok_or_else(err)?;
    let head=decode_header(token).map_err(|_|err())?;
    if head.alg!=Algorithm::RS256{return Err(err())}
    let kid=head.kid.ok_or_else(err)?;
    let mut cache=app.jwks.lock().await;
    if cache.as_ref().is_none_or(|(t,k)|t.elapsed()>Duration::from_secs(300)||k.find(&kid).is_none()){
        // Bound key refreshes on unknown kid as well as normal cache expiry.
        if cache.as_ref().is_some_and(|(t,k)|t.elapsed()<Duration::from_secs(5)&&k.find(&kid).is_none()){return Err(err())}
        let keys=app.http.get(&app.jwks_url).send().await.map_err(|_|err())?.error_for_status().map_err(|_|err())?.json::<JwkSet>().await.map_err(|_|err())?;
        *cache=Some((Instant::now(),keys));
    }
    let jwk=cache.as_ref().and_then(|(_,k)|k.find(&kid)).ok_or_else(err)?;
    let key=DecodingKey::from_jwk(jwk).map_err(|_|err())?;drop(cache);
    let mut v=Validation::new(Algorithm::RS256);
    v.set_audience(&[RESOURCE]);v.set_issuer(&[ISSUER]);v.set_required_spec_claims(&["exp","iat","iss","aud","sub"]);v.leeway=0;
    let claims=decode::<Claims>(token,&key,&v).map_err(|_|err())?.claims;
    if claims.iss!=ISSUER||claims.typ!="Bearer"||!CLIENTS.contains(&claims.azp.as_str())||claims.sub.is_empty()||!claims.scope.split_whitespace().any(|s|s=="gis:live")||claims.exp<=unix_now()||claims.exp<=claims.iat||claims.exp-claims.iat>3600||claims.iat>unix_now().saturating_add(30){return Err(err())}
    Ok(claims)
}
fn browser_guard(h:&HeaderMap)->Result<String,Failure>{
    if !trusted_host(h)||hdr(h,"origin")!=ORIGIN||hdr(h,"x-ovg-ai")!="1"{return Err(Failure::new(403,"Trusted GIS origin required"))}
    let cookies=hdr(h,"cookie");let mut values=cookies.split(';').filter_map(|c|c.trim().strip_prefix("ovg_session="));
    let cookie=values.next().filter(|v|!v.is_empty()&&v.len()<=1024).ok_or(Failure::new(401,"Sign in to OpenVietGIS"))?;
    if values.next().is_some()||cookie.chars().any(|c|c.is_control()||c.is_whitespace()){return Err(Failure::new(401,"Invalid GIS session"))}
    Ok(cookie.to_string())
}
async fn identity(app:&App,cookie:&str,org:Uuid,project:Uuid)->Result<Owner,Failure>{
    async fn get(app:&App,cookie:&str,path:&str)->Result<Value,Failure>{
        let r=app.http.get(format!("{}{path}",app.core)).header(header::COOKIE,format!("ovg_session={cookie}")).send().await.map_err(|_|Failure::new(403,"GIS identity unavailable"))?;
        if r.status()!=StatusCode::OK{return Err(Failure::new(if r.status()==StatusCode::UNAUTHORIZED{401}else{403},"GIS permission unavailable"))}
        r.json().await.map_err(|_|Failure::new(403,"GIS identity verification failed"))
    }
    let access_path=format!("/organizations/{org}/projects/{project}/access/mine");
    let (me,links,access)=tokio::try_join!(get(app,cookie,"/me"),get(app,cookie,"/me/identity-links"),get(app,cookie,&access_path))?;
    if me["status"]!="active"||access["restricted"]!=false{return Err(Failure::new(403,"Unrestricted project read access required"))}
    let subjects:HashSet<_>=links.as_array().ok_or(Failure::new(403,"Identity links unavailable"))?.iter().filter(|x|x["issuer"]==ISSUER).filter_map(|x|x["subject"].as_str()).collect();
    if subjects.len()!=1{return Err(Failure::new(403,"Exactly one linked Keycloak identity required"))}
    let subject=subjects.into_iter().next().unwrap_or_default().to_string();
    let user_id=uuid(&me["id"]).map_err(|_|Failure::new(403,"Invalid GIS identity"))?;
    Ok(Owner{user_id,subject,organization_id:org,project_id:project})
}
async fn open(State(app):State<App>,h:HeaderMap,body:Bytes)->Result<Response,Failure>{
    let cookie=browser_guard(&h)?;let v=parse(&body,&h)?;
    let org=uuid(&v["organization_id"])?;let project=uuid(&v["project_id"])?;
    let version=v.get("workspace_version").and_then(Value::as_u64).filter(|v|*v==1||*v==2).unwrap_or(0) as u8;
    let owner=identity(&app,&cookie,org,project).await?;
    let mut store=app.store()?;let mode=v.get("workspace_mode").and_then(Value::as_str).unwrap_or("gis");if !["gis","cad","3d"].contains(&mode){return Err(Failure::new(422,"Invalid workspace mode"))}let result=store.open(owner.clone(),version,app.now())?;store.set_mode(result.session_id,&result.browser_token,&owner,mode,app.now())?;
    Ok(respond(201,json!(result)))
}
async fn browser_identity(app:&App,h:&HeaderMap,id:Uuid)->Result<(Owner,String),Failure>{
    let cookie=browser_guard(h)?;let token=hdr(h,"x-ovg-bridge").to_string();
    let old=app.store()?.browser_owner(id,&token,app.now())?;
    let fresh=identity(app,&cookie,old.organization_id,old.project_id).await?;
    if fresh!=old{return Err(Failure::new(403,"Workspace authorization changed"))}
    Ok((fresh,token))
}
async fn browser_action(State(app):State<App>,Path((id,action)):Path<(Uuid,String)>,h:HeaderMap,body:Bytes)->Result<Response,Failure>{
    let v=parse(&body,&h)?;let(owner,token)=browser_identity(&app,&h,id).await?;
    let now=app.now();let mut s=app.store()?;
    if let Some(mode)=v.get("workspace_mode").and_then(Value::as_str){s.set_mode(id,&token,&owner,mode,now)?;}
    let r=match action.as_str(){
        "ping"=>{s.ping(id,&token,&owner,now)?;json!({"alive":true})},
        "poll"=>{let c=s.poll(id,&token,&owner,now)?;json!({"command":c.map(|c|json!({"id":c.id,"method":c.method,"params":c.params,"expires_in":c.expires_at_ms.saturating_sub(now) as f64/1000.0}))})},
        "close"=>{s.close(id,&token,&owner,now)?;json!({"closed":true})},
        _=>return Err(Failure::new(404,"Unknown action"))
    };Ok(respond(200,r))
}
async fn browser_command(State(app):State<App>,Path((id,cid,action)):Path<(Uuid,Uuid,String)>,h:HeaderMap,body:Bytes)->Result<Response,Failure>{
    let v=parse(&body,&h)?;let(owner,token)=browser_identity(&app,&h,id).await?;
    let now=app.now();let mut s=app.store()?;
    if let Some(mode)=v.get("workspace_mode").and_then(Value::as_str){s.set_mode(id,&token,&owner,mode,now)?;}
    let r=match action.as_str(){
        "prepare"=>json!({"authorized":true,"expires_in":s.prepare(id,&token,&owner,cid,now)? as f64/1000.0}),
        "ack"=>{let ok=v["ok"].as_bool().ok_or(Failure::new(422,"Invalid acknowledgement"))?;s.ack(id,&token,&owner,cid,ok,v["result"].clone(),now)?;json!({"accepted":true})},
        _=>return Err(Failure::new(404,"Unknown action"))
    };Ok(respond(200,r))
}
async fn metadata()->Response{respond(200,json!({"resource":RESOURCE,"authorization_servers":[ISSUER],"scopes_supported":["gis:live"],"bearer_methods_supported":["header"]}))}
fn rpc(id:Value,v:Value)->Response{respond(200,json!({"jsonrpc":"2.0","id":id,"result":v}))}
fn rpc_error(id:Value,code:i64,msg:&str)->Response{respond(200,json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":msg}}))}
fn tool_result(v:Value,error:bool)->Value{json!({"content":[{"type":"text","text":v.to_string()}],"structuredContent":v,"isError":error})}
async fn operate(app:&App,claims:&Claims,name:&str,args:&Value)->Result<Value,Failure>{
    operate_subject(app,&claims.sub,claims.exp,name,args).await
}
async fn operate_subject(app:&App,subject:&str,expires:u64,name:&str,args:&Value)->Result<Value,Failure>{
    let(target,params)=app.registry.call(name,args)?;
    if expires<=unix_now(){return Err(Failure::new(401,"OAuth token expired"))}
    if name=="gis_workspaces"{return Ok(json!({"workspaces":app.store()?.list(subject,app.now())}))}
    if name=="gis_operation_status"{
        let ticket=uuid(&params["operation_ticket"])?;let now=app.now();let mut s=app.store()?;
        let id=s.resolve(subject,target,now)?;return Ok(json!(s.result(subject,id,ticket,now)?))
    }
    let(method,version,ttl)=dispatch(name);let now=app.now();
    let deadline=now.saturating_add(expires.saturating_sub(unix_now()).saturating_mul(1000));
    let(id,cid)=app.store()?.submit(subject,target,method,params,version,ttl,deadline,now)?;
    if ttl>40_000{return Ok(json!({"state":"queued","session_id":id,"operation_ticket":cid,"expires_in":ttl.min(deadline.saturating_sub(now))/1000,"next":"Wait for browser approval; call gis_operation_status with the same session_id and ticket. Do not resubmit."}))}
    // Cancellation of the HTTP future also cancels an uncompleted command.
    struct Cancel<'a>{app:&'a App,subject:&'a str,id:Uuid,cid:Uuid}
    impl Drop for Cancel<'_>{fn drop(&mut self){if let Ok(mut s)=self.app.store(){let _=s.cancel(self.subject,self.id,self.cid,self.app.now());}}}
    let _guard=Cancel{app,subject:subject,id,cid};
    loop{
        if expires<=unix_now(){return Err(Failure::new(401,"OAuth token expired"))}
        let r=app.store()?.result(subject,id,cid,app.now())?;
        match r.state{
            CommandState::Done=>return Ok(json!({"acknowledged":true,"result":r.result})),
            CommandState::Denied|CommandState::Expired=>return Err(Failure::new(409,"GIS user declined or command expired")),
            _=>{}
        }
        if app.now().saturating_sub(now)>43_000{return Err(Failure::new(409,"GIS acknowledgement timed out"))}
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
/// Wave 2 native host bridge. Call only with claims verified by the OAuth boundary.
/// Wave 3 supplies the provider and user-facing entry; no new remote route here.
#[allow(dead_code)]
async fn run_workspace_agent<M, MF>(app:&App,claims:&Claims,session:Uuid,allowed:Vec<String>,skill:Option<&str>,user:String,cancel:tokio::sync::watch::Receiver<bool>,model:M)->Result<openvietgis_mcp_rust::agent::Report,Failure>
where M:FnMut(openvietgis_mcp_rust::agent::ModelRequest)->MF, MF:std::future::Future<Output=Result<openvietgis_mcp_rust::agent::Reply,openvietgis_mcp_rust::agent::ProviderError>>{
    if claims.exp<=unix_now(){return Err(Failure::new(401,"OAuth token expired"))}
    app.store()?.resolve(&claims.sub,Some(session),app.now())?;
    let mut agent=openvietgis_mcp_rust::agent::Agent::new(&app.registry,session,allowed,Duration::from_secs(300));
    if let Some(name)=skill{agent=agent.with_skill(name).map_err(|_|Failure::new(422,"Unknown GIS skill"))?;}
    Ok(agent.run(user,cancel,model,|call|async move{
        match operate(app,claims,&call.name,&call.arguments).await{
            Ok(value)=>openvietgis_mcp_rust::agent::from_dispatch(&call,value),
            Err(_)=>openvietgis_mcp_rust::agent::ToolOutcome::Failed
        }
    }).await)
}

// Wave 3: opt-in first-party model gateway. Disabled unless the operator allowlists
// a subject and configures a provider. Never accept a caller-supplied URL or key.
fn provider_allowed(subject:&str)->bool{
    std::env::var("OVG_AI_ALLOWED_SUBJECTS").ok().is_some_and(|s|s.split(',').any(|x|x.trim()==subject&&!subject.is_empty()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentInput {provider:String, expected_model:String, user:String, skill:Option<String>, consent:bool, allow_writes:bool}
async fn provider_list(State(app):State<App>,Path(id):Path<Uuid>,h:HeaderMap,body:Bytes)->Result<Response,Failure>{
    let v=parse(&body,&h)?;if v!=json!({}){return Err(Failure::new(422,"Invalid provider request"))}
    let(owner,_)=browser_identity(&app,&h,id).await?;
    let providers=if provider_allowed(&owner.subject){openvietgis_mcp_rust::provider::Provider::ALL.into_iter().filter_map(openvietgis_mcp_rust::provider::Config::from_env).map(|c|c.descriptor()).collect::<Vec<_>>()}else{vec![]};
    Ok(respond(200,json!({"providers":providers})))
}
fn active_agents()->&'static Mutex<HashSet<String>>{
    static ACTIVE:std::sync::OnceLock<Mutex<HashSet<String>>>=std::sync::OnceLock::new();
    ACTIVE.get_or_init(||Mutex::new(HashSet::new()))
}
struct AgentSlot(String);
impl AgentSlot{
    fn acquire(subject:&str)->Result<Self,Failure>{let mut a=active_agents().lock().map_err(|_|Failure::new(503,"Agent unavailable"))?;if a.len()>=4||!a.insert(subject.into()){return Err(Failure::new(429,"Agent capacity reached"))}Ok(Self(subject.into()))}
}
impl Drop for AgentSlot{fn drop(&mut self){if let Ok(mut a)=active_agents().lock(){a.remove(&self.0);}}}
async fn browser_agent(State(app):State<App>,Path(id):Path<Uuid>,h:HeaderMap,body:Bytes)->Result<Response,Failure>{
    use openvietgis_mcp_rust::{agent::{Agent,ProviderError,ToolOutcome,from_dispatch},provider::{Provider,Config,Gateway}};
    let v=parse(&body,&h)?;let input:AgentInput=serde_json::from_value(v).map_err(|_|Failure::new(422,"Invalid agent request"))?;
    if !input.consent||input.user.trim().is_empty()||input.user.len()>16000{return Err(Failure::new(422,"Explicit provider consent and request required"))}
    let(owner,_)=browser_identity(&app,&h,id).await?;
    if !provider_allowed(&owner.subject){return Err(Failure::new(403,"Model gateway not enabled for this account"))}
    let p=Provider::parse(&input.provider).ok_or(Failure::new(422,"Unsupported provider"))?;
    let cfg=Config::from_env(p).ok_or(Failure::new(503,"Provider not configured"))?;
    if cfg.descriptor()["model"]!=input.expected_model.as_str(){return Err(Failure::new(409,"Provider model changed; refresh consent"))}
    let _slot=AgentSlot::acquire(&owner.subject)?;
    let gateway=Gateway::new(cfg).map_err(|_|Failure::new(503,"Provider unavailable"))?;
    let allowed=app.registry.list().iter().filter(|t|{
        let name=t["name"].as_str().unwrap_or("");
        // Administration remains excluded even when the user permits editing.
        !["gis_plugin_activate","gis_plugin_disable","gis_plugin_create","gis_plugin_update","gis_plugin_records"].contains(&name)
        && (input.allow_writes||t["annotations"]["readOnlyHint"]==true)
    }).filter_map(|t|t["name"].as_str().map(str::to_owned)).collect::<Vec<_>>();
    let mut agent=Agent::new(&app.registry,id,allowed,Duration::from_secs(300));
    if let Some(skill)=input.skill.as_deref(){agent=agent.with_skill(skill).map_err(|_|Failure::new(422,"Unknown GIS skill"))?;}
    let (_cancel_tx,cancel)=tokio::sync::watch::channel(false);
    let deadline=unix_now().saturating_add(300);
    let app_ref=&app;let headers=&h;let expected=&owner;let model_gateway=&gateway;
    let report=agent.run(input.user,cancel,|request|async move{
        // Recheck identity before every outbound transmission, including follow-up observations.
        if unix_now()>=deadline||!provider_allowed(&expected.subject){return Err(ProviderError::Fatal)}
        match browser_identity(app_ref,headers,id).await{Ok((fresh,_)) if fresh==*expected=>model_gateway.complete(request).await,_=>Err(ProviderError::Fatal)}
    },|call|async move{
        match browser_identity(app_ref,headers,id).await{
            Ok((fresh,_)) if fresh==*expected=>match operate_subject(app_ref,&expected.subject,deadline,&call.name,&call.arguments).await{Ok(value)=>from_dispatch(&call,value),Err(_)=>ToolOutcome::Failed},
            _=>ToolOutcome::Failed
        }
    }).await;
    // Do not return raw features, provider-native thinking, or credentials. The browser sees
    // final text and operation status only, existing review/history shows the GIS commands.
    Ok(respond(200,json!({"stop":report.stop,"answer":report.answer,"model_steps":report.model_steps,"calls":report.calls,"unresolved_ticket":report.unresolved_ticket,"uncertain_call":report.uncertain_call})))
}

async fn mcp(State(app):State<App>,h:HeaderMap,body:Bytes)->Response{
    if !trusted_host(&h)||!origin_ok(&h){return respond(403,json!({"error":"Untrusted origin"}))}
    let claims=match verify(&app,&h).await{Ok(c)=>c,Err(_)=>return unauthorized()};
    let v=match parse(&body,&h){Ok(v)=>v,Err(e)=>return e.into_response()};
    let id=v.get("id").cloned().unwrap_or(Value::Null);
    if v["jsonrpc"]!="2.0"||(!id.is_null()&&!id.is_string()&&!id.is_number()){return rpc_error(id,-32600,"Invalid request")}
    let method=v["method"].as_str().unwrap_or("");
    if method.is_empty(){return rpc_error(id,-32600,"Invalid method")}
    if v.get("id").is_none(){return StatusCode::ACCEPTED.into_response()}
    let version=hdr(&h,"mcp-protocol-version");
    if !version.is_empty()&&!PROTOCOLS.contains(&version){return respond(400,json!({"error":"Unsupported MCP protocol version"}))}
    match method{
        "initialize"=>{let requested=v["params"]["protocolVersion"].as_str().unwrap_or("");let protocol=if PROTOCOLS.contains(&requested){requested}else{PROTOCOLS[3]};rpc(id,json!({"protocolVersion":protocol,"serverInfo":{"name":"OpenVietGIS Live Rust","version":"0.2.0"},"capabilities":{"tools":{"listChanged":false},"prompts":{"listChanged":false}},"instructions":"Discover built-in GIS workflows using prompts/list and prompts/get. Select an explicit workspace when several are connected. All persistent writes require browser approval; never retry a pending operation."}))},
        "ping"=>rpc(id,json!({})),
        "prompts/list"=>{if v.get("params").is_some_and(|p| !p.as_object().is_some_and(|o| o.is_empty())){rpc_error(id,-32602,"Invalid prompt list parameters")}else{rpc(id,openvietgis_mcp_rust::skills::list())}},
        "prompts/get"=>match openvietgis_mcp_rust::skills::get(&v["params"]){Some(p)=>rpc(id,p),None=>rpc_error(id,-32602,"Unknown prompt or invalid arguments")},
        "tools/list"=>rpc(id,json!({"tools":app.registry.list()})),
        "tools/call"=>{let name=v["params"]["name"].as_str().unwrap_or("");let args=v["params"].get("arguments").cloned().unwrap_or(json!({}));
            match operate(&app,&claims,name,&args).await{
                Ok(out)=>rpc(id,tool_result(out,false)),
                Err(e)=>rpc(id,tool_result(json!({"ok":false,"status":e.status,"message":e.msg}),true))
            }},
        _=>rpc_error(id,-32601,"Method not found")
    }
}
async fn mcp_get(State(app):State<App>,h:HeaderMap)->Response{
    if !trusted_host(&h)||!origin_ok(&h){return respond(403,json!({"error":"Untrusted origin"}))}
    if verify(&app,&h).await.is_err(){return unauthorized()}
    StatusCode::METHOD_NOT_ALLOWED.into_response()
}
fn app(core:String,jwks_url:String)->Result<App,reqwest::Error>{
    Ok(App{store:Arc::new(Mutex::new(Store::default())),registry:Arc::new(Registry::new()),http:reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(5)).build()?,clock:Instant::now(),jwks:Arc::new(tokio::sync::Mutex::new(None)),core,jwks_url})
}
fn routes(app:App)->Router{
    Router::new()
        .route("/.well-known/oauth-protected-resource/mcp",get(metadata))
        .route("/mcp",post(mcp).get(mcp_get))
        .route("/ai-copilot/sessions",post(open))
        .route("/ai-copilot/sessions/{id}/{action}",post(browser_action))
        .route("/ai-copilot/agent/{id}/providers",post(provider_list))
        .route("/ai-copilot/agent/{id}/run",post(browser_agent))
        .route("/ai-copilot/sessions/{id}/commands/{cid}/{action}",post(browser_command))
        .layer(DefaultBodyLimit::max(128_000))
        .with_state(app)
}
#[tokio::main]
async fn main()->Result<(),Box<dyn std::error::Error>>{
    let app=app(CORE.into(),JWKS.into())?;
    // Separate loopback candidate port, never shadow the production relay.
    let listener=tokio::net::TcpListener::bind("127.0.0.1:9188").await?;
    println!("native MCP candidate listening on loopback :9188");
    axum::serve(listener,routes(app)).await?;Ok(())
}
#[cfg(test)]mod tests{
    use super::*;
    use base64::{Engine as _,engine::general_purpose::URL_SAFE_NO_PAD};
    use rsa::{RsaPrivateKey,traits::PublicKeyParts,pkcs1::EncodeRsaPrivateKey};
    use jsonwebtoken::{encode,Header,EncodingKey};

    #[test]fn duplicate_json_rejected(){
        let mut h=HeaderMap::new();h.insert(header::CONTENT_TYPE,header::HeaderValue::from_static("application/json"));
        assert!(parse(&Bytes::from_static(br#"{"x":1,"x":2}"#),&h).is_err());
        assert!(parse(&Bytes::from_static(br#"{"x":{"a":1,"a":2}}"#),&h).is_err());
        assert!(parse(&Bytes::from_static(br#"{"x":1}"#),&h).is_ok());
    }

    async fn serve(router:Router)->(String,tokio::task::JoinHandle<()>){
        let l=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url=format!("http://{}",l.local_addr().unwrap());
        let task=tokio::spawn(async move{axum::serve(l,router).await.unwrap()});
        (url,task)
    }
    fn sign(key:&RsaPrivateKey,claims:Value)->String{
        let mut h=Header::new(Algorithm::RS256);h.kid=Some("test-key".into());
        encode(&h,&claims,&EncodingKey::from_rsa_der(key.to_pkcs1_der().unwrap().as_bytes())).unwrap()
    }
    fn claims(subject:&str)->Value{
        let t=unix_now();json!({"iss":ISSUER,"sub":subject,"aud":RESOURCE,"azp":CLIENTS[0],"scope":"openid gis:live","typ":"Bearer","iat":t,"exp":t+900})
    }
    async fn post_rpc(client:&reqwest::Client,url:&str,token:&str,method:&str,params:Value)->reqwest::Response{
        client.post(format!("{url}/mcp")).header("host","openvietgis.cloud").bearer_auth(token).json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})).send().await.unwrap()
    }
    async fn browser(client:&reqwest::Client,url:&str,path:&str,token:Option<&str>,data:Value)->reqwest::Response{
        let req=client.post(format!("{url}/ai-copilot/sessions{path}")).header("host","openvietgis.cloud").header("origin",ORIGIN).header("x-ovg-ai","1").header("cookie","ovg_session=test-cookie");
        let req=if let Some(t)=token{req.header("x-ovg-bridge",t)}else{req};req.json(&data).send().await.unwrap()
    }
    #[tokio::test]
    async fn signed_oauth_and_multi_project_browser_roundtrip(){
        // Ephemeral fixture key; no live OAuth credentials or production calls.
        let key=RsaPrivateKey::new(&mut rand::thread_rng(),2048).unwrap();
        let jwks=json!({"keys":[{"kty":"RSA","kid":"test-key","alg":"RS256","use":"sig","n":URL_SAFE_NO_PAD.encode(key.n().to_bytes_be()),"e":URL_SAFE_NO_PAD.encode(key.e().to_bytes_be())}]});
        let uid=Uuid::new_v4();let org=Uuid::new_v4();let project_a=Uuid::new_v4();let project_b=Uuid::new_v4();
        let core_router=Router::new()
            .route("/jwks",get(move||{let j=jwks.clone();async move{Json(j)}}))
            .route("/api/v1/me",get(move|h:HeaderMap|async move{if hdr(&h,"cookie")!="ovg_session=test-cookie"{return respond(401,json!({}))}respond(200,json!({"id":uid,"status":"active"}))}))
            .route("/api/v1/me/identity-links",get(||async{Json(json!([{"issuer":ISSUER,"subject":"alice"}]))}))
            .route("/api/v1/organizations/{org}/projects/{project}/access/mine",get(move|Path((o,p)):Path<(Uuid,Uuid)>|async move{Json(json!({"restricted":o!=org||(p!=project_a&&p!=project_b)}))}));
        let (core,core_task)=serve(core_router).await;
        let service=app(format!("{core}/api/v1"),format!("{core}/jwks")).unwrap();
        let (url,task)=serve(routes(service.clone())).await;
        let client=reqwest::Client::new();
        let valid=sign(&key,claims("alice"));
        let mut wrong=claims("alice");wrong["aud"]=json!("other-resource");
        assert_eq!(post_rpc(&client,&url,&sign(&key,wrong),"tools/list",json!({})).await.status(),401);
        let mut wrong=claims("alice");wrong["scope"]=json!("openid");
        assert_eq!(post_rpc(&client,&url,&sign(&key,wrong),"tools/list",json!({})).await.status(),401);
        let mut wrong=claims("alice");wrong["azp"]=json!("openvietgis-web");
        assert_eq!(post_rpc(&client,&url,&sign(&key,wrong),"tools/list",json!({})).await.status(),401);
        assert_eq!(post_rpc(&client,&url,"invalid","tools/list",json!({})).await.status(),401);
        let init:Value=post_rpc(&client,&url,&valid,"initialize",json!({"protocolVersion":"2026-07-28"})).await.json().await.unwrap();
        assert_eq!(init["result"]["protocolVersion"],"2026-07-28");
        assert_eq!(init["result"]["capabilities"]["prompts"]["listChanged"],false);
        let prompts:Value=post_rpc(&client,&url,&valid,"prompts/list",json!({})).await.json().await.unwrap();
        assert_eq!(prompts["result"]["prompts"].as_array().unwrap().len(),6);
        let skill:Value=post_rpc(&client,&url,&valid,"prompts/get",json!({"name":"explore-data"})).await.json().await.unwrap();
        assert_eq!(skill["result"]["messages"][0]["content"]["type"],"text");
        let invalid:Value=post_rpc(&client,&url,&valid,"prompts/get",json!({"name":"missing"})).await.json().await.unwrap();
        assert_eq!(invalid["error"]["code"],-32602);
        assert_eq!(post_rpc(&client,&url,"invalid","prompts/list",json!({})).await.status(),401);
        let legacy:Value=post_rpc(&client,&url,&valid,"initialize",json!({"protocolVersion":"2025-11-25"})).await.json().await.unwrap();
        assert_eq!(legacy["result"]["protocolVersion"],"2025-11-25");
        let listed:Value=post_rpc(&client,&url,&valid,"tools/list",json!({})).await.json().await.unwrap();
        assert_eq!(listed["result"]["tools"].as_array().unwrap().len(),39);
        let denied=browser(&client,&url,"",None,json!({"organization_id":org,"project_id":Uuid::new_v4(),"workspace_version":2})).await;
        assert_eq!(denied.status(),403);
        let a:Value=browser(&client,&url,"",None,json!({"organization_id":org,"project_id":project_a,"workspace_version":2})).await.json().await.unwrap();
        let b:Value=browser(&client,&url,"",None,json!({"organization_id":org,"project_id":project_b,"workspace_version":2})).await.json().await.unwrap();
        let sa=a["session_id"].as_str().unwrap();let sb=b["session_id"].as_str().unwrap();
        let ta=a["browser_token"].as_str().unwrap();let tb=b["browser_token"].as_str().unwrap();
        let many:Value=post_rpc(&client,&url,&valid,"tools/call",json!({"name":"gis_workspaces","arguments":{}})).await.json().await.unwrap();
        assert_eq!(many["result"]["structuredContent"]["workspaces"].as_array().unwrap().len(),2);
        let amb:Value=post_rpc(&client,&url,&valid,"tools/call",json!({"name":"gis_live_view","arguments":{}})).await.json().await.unwrap();
        assert_eq!(amb["result"]["structuredContent"]["status"],409);
        let other=sign(&key,claims("mallory"));
        let other_result:Value=post_rpc(&client,&url,&other,"tools/call",json!({"name":"gis_live_view","arguments":{"session_id":sb}})).await.json().await.unwrap();
        assert_eq!(other_result["result"]["structuredContent"]["status"],404);
        let (c,u,t,id)=(client.clone(),url.clone(),valid.clone(),sb.to_string());
        let call=tokio::spawn(async move{post_rpc(&c,&u,&t,"tools/call",json!({"name":"gis_live_view","arguments":{"session_id":id}})).await.json::<Value>().await.unwrap()});
        tokio::time::sleep(Duration::from_millis(100)).await;
        let no:Value=browser(&client,&url,&format!("/{sa}/poll"),Some(ta),json!({})).await.json().await.unwrap();
        assert!(no["command"].is_null());
        let wrong=browser(&client,&url,&format!("/{sb}/poll"),Some(ta),json!({})).await;assert_eq!(wrong.status(),404);
        let cmd:Value=browser(&client,&url,&format!("/{sb}/poll"),Some(tb),json!({})).await.json().await.unwrap();
        let cid=cmd["command"]["id"].as_str().unwrap();
        assert_eq!(cmd["command"]["method"],"map_view");
        let prep:Value=browser(&client,&url,&format!("/{sb}/commands/{cid}/prepare"),Some(tb),json!({})).await.json().await.unwrap();
        assert_eq!(prep["authorized"],true);
        let ack=browser(&client,&url,&format!("/{sb}/commands/{cid}/ack"),Some(tb),json!({"ok":true,"result":{"crs":"EPSG:4326","center":[1,2],"bbox":[0,0,3,3],"zoom":6,"unapproved_extra":"hidden"}})).await;
        assert_eq!(ack.status(),200);
        let result=call.await.unwrap();assert_eq!(result["result"]["structuredContent"]["acknowledged"],true);
        assert!(result["result"]["structuredContent"]["result"].get("unapproved_extra").is_none());
        use openvietgis_mcp_rust::agent::{Reply,ToolCall,Stop};
        let auth=verify(&service,&{
            let mut headers=HeaderMap::new();
            headers.insert(header::AUTHORIZATION,format!("Bearer {valid}").parse().unwrap());headers
        }).await.unwrap();
        let scoped=Uuid::parse_str(sa).unwrap();
        let (_cancel_tx,cancel)=tokio::sync::watch::channel(false);
        let mut turns=0;
        let run=run_workspace_agent(&service,&auth,scoped,vec!["gis_live_view".into()],None,"inspect map".into(),cancel,|_|{
            turns+=1;
            std::future::ready(Ok(if turns==1{Reply{text:String::new(),calls:vec![ToolCall{id:"view".into(),name:"gis_live_view".into(),arguments:json!({})}]}}else{Reply{text:"verified".into(),calls:vec![]}}))
        });
        let browser_driver=async{
            tokio::time::sleep(Duration::from_millis(100)).await;
            let cmd:Value=browser(&client,&url,&format!("/{sa}/poll"),Some(ta),json!({})).await.json().await.unwrap();
            let cid=cmd["command"]["id"].as_str().unwrap();
            assert_eq!(cmd["command"]["method"],"map_view");
            let prep=browser(&client,&url,&format!("/{sa}/commands/{cid}/prepare"),Some(ta),json!({})).await;
            assert_eq!(prep.status(),200);
            let ack=browser(&client,&url,&format!("/{sa}/commands/{cid}/ack"),Some(ta),json!({"ok":true,"result":{"crs":"EPSG:4326","center":[1,2],"bbox":[0,0,3,3],"zoom":6}})).await;
            assert_eq!(ack.status(),200);
        };
        let (report,())=tokio::join!(run,browser_driver);
        let report=report.unwrap();assert_eq!(report.stop,Stop::Complete);assert_eq!(report.calls,1);
        task.abort();core_task.abort();
    }
}
