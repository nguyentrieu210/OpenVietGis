//! Standalone ACP runner, ONLY for a dedicated isolated worker; never start on GIS VPS.
//! Operator installs and pins each executable and enforces a network/filesystem sandbox.
//! No shell, no client-supplied args, no GIS credentials, no auto-install.
use axum::{Router,extract::{State,DefaultBodyLimit},http::{HeaderMap,StatusCode},routing::post,Json};
use openvietgis_mcp_rust::acp::{self,Input,Report,Session,Event};
use serde_json::{json,Value};
use std::{collections::HashMap,io::{BufRead,BufReader,Write,Read},path::PathBuf,process::{Command,Stdio},sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},time::{Instant,Duration}};
use subtle::ConstantTimeEq;
use uuid::Uuid;
#[derive(Clone)]struct App{key:Arc<String>,work:PathBuf,active:Arc<Mutex<HashMap<Uuid,Arc<AtomicBool>>>>}
fn auth(a:&App,h:&HeaderMap)->bool{
    h.get("authorization").and_then(|v|v.to_str().ok()).and_then(|v|v.strip_prefix("Bearer ")).is_some_and(|v|v.as_bytes().ct_eq(a.key.as_bytes()).into())
}
fn executable(agent:&str)->Option<(PathBuf,&'static [&'static str])>{
    let (name,args):(&str,&[&str])=match agent{
        "codex"=>("CODEX",&[]),"claude-code"=>("CLAUDE_CODE",&[]),"gemini"=>("GEMINI",&["--acp"]),"copilot"=>("COPILOT",&["--acp"]),"cursor"=>("CURSOR",&["acp"]),"opencode"=>("OPENCODE",&["acp"]),"goose"=>("GOOSE",&["acp"]),_=>return None
    };
    let path=PathBuf::from(std::env::var(format!("OVG_ACP_EXEC_{name}")).ok()?);
    if !path.is_absolute()||!path.is_file(){return None}Some((path,args))
}
struct ChildGuard(std::process::Child);
impl Drop for ChildGuard{fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
struct WorkGuard(PathBuf);
impl Drop for WorkGuard{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn execute(input:Input,root:PathBuf,cancel:Arc<AtomicBool>)->Report{
    let Some((exe,args))=executable(&input.agent)else{return Report::failed("runner_unavailable")};
    let path=root.join(input.run_id.to_string());
    if std::fs::create_dir(&path).is_err(){return Report::failed("runner_unavailable")}
    let _work=WorkGuard(path.clone());
    // No inherited GIS/API environment. Adapter authentication must be configured by
    // operator inside the dedicated sandbox, not supplied by caller.
    let mut cmd=Command::new(exe);
    cmd.args(args).current_dir(&path).env_clear().env("PATH","/usr/local/bin:/usr/bin:/bin").env("HOME",&path).env("TMPDIR",&path).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let Ok(child)=cmd.spawn()else{return Report::failed("runner_unavailable")};let mut child=ChildGuard(child);
    let Some(mut stdin)=child.0.stdin.take()else{return Report::failed("runner_unavailable")};
    let Some(stdout)=child.0.stdout.take()else{return Report::failed("runner_unavailable")};
    let (tx,rx)=std::sync::mpsc::sync_channel::<Option<String>>(8);
    std::thread::spawn(move||{
        let mut reader=BufReader::new(stdout);
        loop{
            let mut buf=Vec::new();
            match reader.by_ref().take((acp::MAX_FRAME+2) as u64).read_until(b'\n',&mut buf){
                Ok(n) if n>0&&n<=acp::MAX_FRAME+1&&buf.ends_with(b"\n")=>{match String::from_utf8(buf){Ok(s)=>if tx.send(Some(s)).is_err(){break},Err(_)=>{let _=tx.send(None);break}}},
                _=>{let _=tx.send(None);break}
            }
        }
    });
    // Writes run on a bounded channel so a child refusing stdin cannot defeat deadline.
    let (send,recv)=std::sync::mpsc::sync_channel::<Value>(8);
    std::thread::spawn(move||{while let Ok(v)=recv.recv(){if writeln!(stdin,"{}",v).and_then(|_|stdin.flush()).is_err(){break}}});
    let(mut session,init)=Session::new(input.task,path.to_string_lossy().into_owned());
    if send.try_send(init).is_err(){return Report::failed("runner_unavailable")}
    let deadline=Instant::now()+Duration::from_secs(120);
    loop{
        if cancel.load(Ordering::SeqCst)||Instant::now()>=deadline{
            if let Some(v)=session.cancel(){let _=send.try_send(v);}
            return Report::failed(if cancel.load(Ordering::SeqCst){"cancelled"}else{"timeout"})
        }
        match rx.recv_timeout(Duration::from_millis(100)){
            Ok(Some(line))=>match session.line(&line){
                Event::Send(v)=>if send.try_send(v).is_err(){return Report::failed("protocol_error")},
                Event::Done(r)=>return r,Event::Ignore=>{}
            },
            Ok(None)|Err(std::sync::mpsc::RecvTimeoutError::Disconnected)=>return Report::failed("agent_error"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)=>{}
        }
    }
}
async fn run(State(a):State<App>,h:HeaderMap,Json(input):Json<Input>)->Result<Json<Report>,StatusCode>{
    if !auth(&a,&h){return Err(StatusCode::UNAUTHORIZED)}
    if !input.valid()||!acp::Remote::agents().contains(&input.agent){return Err(StatusCode::UNPROCESSABLE_ENTITY)}
    let id=input.run_id;let cancel=Arc::new(AtomicBool::new(false));
    {let mut active=a.active.lock().map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;if !active.is_empty(){return Err(StatusCode::TOO_MANY_REQUESTS)}active.insert(id,cancel.clone());}
    // Move ownership of slot cleanup into the blocking worker: disconnecting HTTP does
    // not free capacity until the child process really exits.
    let active=a.active.clone();let root=a.work.clone();
    let worker=tokio::task::spawn_blocking(move||{
        struct Slot(Arc<Mutex<HashMap<Uuid,Arc<AtomicBool>>>>,Uuid);
        impl Drop for Slot{fn drop(&mut self){if let Ok(mut a)=self.0.lock(){a.remove(&self.1);}}}
        let _slot=Slot(active,id);execute(input,root,cancel)
    });
    Ok(Json(worker.await.unwrap_or_else(|_|Report::failed("runner_unavailable"))))
}
#[derive(serde::Deserialize)]#[serde(deny_unknown_fields)]struct Cancel{run_id:Uuid}
async fn cancel(State(a):State<App>,h:HeaderMap,Json(v):Json<Cancel>)->Result<Json<Value>,StatusCode>{
    if !auth(&a,&h){return Err(StatusCode::UNAUTHORIZED)}
    let active=a.active.lock().map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
    if let Some(flag)=active.get(&v.run_id){flag.store(true,Ordering::SeqCst);}
    Ok(Json(json!({"cancel_requested":true})))
}
#[tokio::main]async fn main(){
    // Explicit deployment interlock; this flag is not evidence of a real sandbox.
    assert_eq!(std::env::var("OVG_ACP_ISOLATED_WORKER").as_deref(),Ok("1"),"Dedicated sandbox required");
    let key=std::env::var("OVG_ACP_RUNNER_KEY").expect("runner key required");assert!(key.len()>=32&&key.len()<=512);
    let work=PathBuf::from(std::env::var("OVG_ACP_WORK_ROOT").expect("work root required"));assert!(work.is_absolute()&&work.is_dir());
    let app=App{key:Arc::new(key),work,active:Arc::new(Mutex::new(HashMap::new()))};
    let router=Router::new().route("/v1/acp/run",post(run)).route("/v1/acp/cancel",post(cancel)).layer(DefaultBodyLimit::max(24000)).with_state(app);
    let listener=tokio::net::TcpListener::bind("127.0.0.1:9199").await.expect("bind isolated runner");axum::serve(listener,router).await.expect("serve runner");
}
