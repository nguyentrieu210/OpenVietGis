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
const PROTOCOLS:[&str;3]=["2025-03-26","2025-06-18","2025-11-25"];
#[derive(Clone)]
struct App {store:Arc<Mutex<Store>>,registry:Arc<Registry>,http:reqwest::Client,clock:Instant,jwks:Arc<tokio::sync::Mutex<Option<(Instant,JwkSet)>>>}
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
        let keys=app.http.get(JWKS).send().await.map_err(|_|err())?.error_for_status().map_err(|_|err())?.json::<JwkSet>().await.map_err(|_|err())?;
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
        let r=app.http.get(format!("{CORE}{path}")).header(header::COOKIE,format!("ovg_session={cookie}")).send().await.map_err(|_|Failure::new(403,"GIS identity unavailable"))?;
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
    let result=app.store()?.open(owner,version,app.now())?;
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
    let _=parse(&body,&h)?;let(owner,token)=browser_identity(&app,&h,id).await?;
    let now=app.now();let mut s=app.store()?;
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
    let(target,params)=app.registry.call(name,args)?;
    if claims.exp<=unix_now(){return Err(Failure::new(401,"OAuth token expired"))}
    if name=="gis_workspaces"{return Ok(json!({"workspaces":app.store()?.list(&claims.sub,app.now())}))}
    if name=="gis_operation_status"{
        let ticket=uuid(&params["operation_ticket"])?;let now=app.now();let mut s=app.store()?;
        let id=s.resolve(&claims.sub,target,now)?;return Ok(json!(s.result(&claims.sub,id,ticket,now)?))
    }
    let(method,version,ttl)=dispatch(name);let now=app.now();
    let deadline=now.saturating_add(claims.exp.saturating_sub(unix_now()).saturating_mul(1000));
    let(id,cid)=app.store()?.submit(&claims.sub,target,method,params,version,ttl,deadline,now)?;
    if ttl>40_000{return Ok(json!({"state":"queued","session_id":id,"operation_ticket":cid,"expires_in":ttl.min(deadline.saturating_sub(now))/1000,"next":"Wait for browser approval; call gis_operation_status with the same session_id and ticket. Do not resubmit."}))}
    // Cancellation of the HTTP future also cancels an uncompleted command.
    struct Cancel<'a>{app:&'a App,subject:&'a str,id:Uuid,cid:Uuid}
    impl Drop for Cancel<'_>{fn drop(&mut self){if let Ok(mut s)=self.app.store(){let _=s.cancel(self.subject,self.id,self.cid,self.app.now());}}}
    let _guard=Cancel{app,subject:&claims.sub,id,cid};
    loop{
        if claims.exp<=unix_now(){return Err(Failure::new(401,"OAuth token expired"))}
        let r=app.store()?.result(&claims.sub,id,cid,app.now())?;
        match r.state{
            CommandState::Done=>return Ok(json!({"acknowledged":true,"result":r.result})),
            CommandState::Denied|CommandState::Expired=>return Err(Failure::new(409,"GIS user declined or command expired")),
            _=>{}
        }
        if app.now().saturating_sub(now)>43_000{return Err(Failure::new(409,"GIS acknowledgement timed out"))}
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
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
        "initialize"=>{let requested=v["params"]["protocolVersion"].as_str().unwrap_or("");let protocol=if PROTOCOLS.contains(&requested){requested}else{PROTOCOLS[2]};rpc(id,json!({"protocolVersion":protocol,"serverInfo":{"name":"OpenVietGIS Live Rust","version":"0.2.0"},"capabilities":{"tools":{"listChanged":false}},"instructions":"Select an explicit workspace when several are connected. All persistent writes require browser approval; never retry a pending operation."}))},
        "ping"=>rpc(id,json!({})),
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
#[tokio::main]
async fn main()->Result<(),Box<dyn std::error::Error>>{
    let app=App{store:Arc::new(Mutex::new(Store::default())),registry:Arc::new(Registry::new()),http:reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(5)).build()?,clock:Instant::now(),jwks:Arc::new(tokio::sync::Mutex::new(None))};
    let routes=Router::new()
        .route("/.well-known/oauth-protected-resource/mcp",get(metadata))
        .route("/mcp",post(mcp).get(mcp_get))
        .route("/ai-copilot/sessions",post(open))
        .route("/ai-copilot/sessions/{id}/{action}",post(browser_action))
        .route("/ai-copilot/sessions/{id}/commands/{cid}/{action}",post(browser_command))
        .layer(DefaultBodyLimit::max(128_000))
        .with_state(app);
    // Separate loopback candidate port, never shadow the production relay.
    let listener=tokio::net::TcpListener::bind("127.0.0.1:9188").await?;
    println!("native MCP candidate listening on loopback :9188");
    axum::serve(listener,routes).await?;Ok(())
}
#[cfg(test)]mod tests{use super::*;#[test]fn duplicate_json_rejected(){let mut h=HeaderMap::new();h.insert(header::CONTENT_TYPE,header::HeaderValue::from_static("application/json"));assert!(parse(&Bytes::from_static(br#"{"x":1,"x":2}"#),&h).is_err());assert!(parse(&Bytes::from_static(br#"{"x":{"a":1,"a":2}}"#),&h).is_err());assert!(parse(&Bytes::from_static(br#"{"x":1}"#),&h).is_ok());}}
