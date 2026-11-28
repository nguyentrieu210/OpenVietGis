//! Fixed typed OpenVietCAD protocol-1 stdio adapter. No raw JSON, script,
//! command, executable, network endpoint or path supplied by the renderer.
//! Process isolation is not an operating-system sandbox.
use super::*;
use std::{io::{BufRead,BufReader,Write},process::{Child,ChildStdin},sync::mpsc::{self,Receiver,RecvTimeoutError}};
const LINE:usize=4*1024*1024;
const ENGINE_TIME:Duration=Duration::from_secs(90);
#[derive(Clone,Copy,Debug,Deserialize)]
#[serde(tag="type",rename_all="snake_case",deny_unknown_fields)]
pub enum EngineEdit{Move{dx:f64,dy:f64},Rotate{x:f64,y:f64,angle:f64},Scale{x:f64,y:f64,factor:f64}}
fn bounded(v:f64)->bool{v.is_finite()&&v.abs()<=1e12}
fn parameters(edit:EngineEdit)->R<Value>{match edit{
 EngineEdit::Move{dx,dy} if bounded(dx)&&bounded(dy)&&(dx!=0.||dy!=0.)=>Ok(serde_json::json!({"action":"move","vector":[dx,dy]})),
 EngineEdit::Rotate{x,y,angle} if bounded(x)&&bounded(y)&&angle.is_finite()&&angle.abs()<=360.&&angle.rem_euclid(360.)!=0.=>Ok(serde_json::json!({"action":"rotate","center":[x,y],"angle_deg":angle})),
 EngineEdit::Scale{x,y,factor} if bounded(x)&&bounded(y)&&factor.is_finite()&&(1e-6..=1e6).contains(&factor.abs())&&factor!=1.=>Ok(serde_json::json!({"action":"scale","center":[x,y],"factor":factor})),
 _=>Err("INVALID_CAD_TRANSFORM")
}}
fn valid_handle(h:&str)->bool{!h.is_empty()&&h.len()<=16&&h.bytes().all(|c|c.is_ascii_hexdigit()&&!c.is_ascii_lowercase())}
struct Engine{child:Child,input:Option<ChildStdin>,output:Receiver<R<Value>>,deadline:Instant}
impl Drop for Engine{fn drop(&mut self){self.input.take();let _=self.child.kill();let _=self.child.wait();}}
impl Engine{
 fn start(exe:&Path,dir:&Path)->R<Self>{
  let mut cmd=Command::new(exe);cmd.arg("--serve").current_dir(dir).env_clear().stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
  for name in ["PATH","SystemRoot","WINDIR","COMSPEC","LANG","LC_ALL"]{if let Some(v)=std::env::var_os(name){cmd.env(name,v);}}
  for name in ["HOME","USERPROFILE","XDG_CONFIG_HOME","XDG_CACHE_HOME","APPDATA","LOCALAPPDATA","TEMP","TMP","TMPDIR"]{cmd.env(name,dir);}
  let mut child=cmd.spawn().map_err(|_|"CAD_ENGINE_START_FAILED")?;
  let input=child.stdin.take().ok_or("CAD_ENGINE_PIPE_FAILED")?;
  let stdout=child.stdout.take().ok_or("CAD_ENGINE_PIPE_FAILED")?;
  let(tx,rx)=mpsc::sync_channel(1);
  std::thread::spawn(move||{
   let mut reader=BufReader::new(stdout);
   loop{
    let mut line=Vec::new();
    let n=match reader.by_ref().take((LINE+1)as u64).read_until(b'\n',&mut line){Ok(n)=>n,Err(_)=>{let _=tx.send(Err("CAD_ENGINE_PIPE_FAILED"));break}};
    if n==0{break}
    if n>LINE||line.last()!=Some(&b'\n'){let _=tx.send(Err("CAD_ENGINE_RESPONSE_LIMIT"));break}
    let v=serde_json::from_slice::<Value>(&line).map_err(|_|"CAD_ENGINE_INVALID_RESPONSE");
    let failed=v.is_err();if tx.send(v).is_err()||failed{break}
   }
  });
  let mut e=Self{child,input:Some(input),output:rx,deadline:Instant::now()+ENGINE_TIME};
  let greeting=e.receive()?;
  if greeting["ok"]!=true||greeting["ready"]!=true||greeting["session_id"].as_str().is_none(){return Err("CAD_ENGINE_GREETING_INVALID")}
  Ok(e)
 }
 fn receive(&mut self)->R<Value>{
  let remaining=self.deadline.checked_duration_since(Instant::now()).ok_or("CAD_ENGINE_TIMEOUT")?;
  match self.output.recv_timeout(remaining){Ok(v)=>v,Err(RecvTimeoutError::Timeout)=>Err("CAD_ENGINE_TIMEOUT"),Err(_)=>Err("CAD_ENGINE_EXITED")}
 }
 fn call(&mut self,request:Value)->R<Value>{
  if Instant::now()>self.deadline{return Err("CAD_ENGINE_TIMEOUT")}
  let mut data=serde_json::to_vec(&request).map_err(|_|"CAD_ENGINE_INVALID_REQUEST")?;
  if data.len()>2048{return Err("CAD_ENGINE_REQUEST_LIMIT")}data.push(b'\n');
  let input=self.input.as_mut().ok_or("CAD_ENGINE_EXITED")?;
  input.write_all(&data).and_then(|_|input.flush()).map_err(|_|"CAD_ENGINE_PIPE_FAILED")?;
  let r=self.receive()?;
  if r["ok"]!=true{return Err("CAD_ENGINE_OPERATION_REFUSED")}
  Ok(r)
 }
 fn mutate(&mut self,document:u64,mut request:Value)->R<Value>{
  let id=Uuid::new_v4().to_string();request["protocol"]=serde_json::json!(1);request["request_id"]=serde_json::json!(id);request["document_id"]=serde_json::json!(document);
  let r=self.call(request)?;
  if r["status"]!="completed"||r["request_id"]!=id{return Err("CAD_ENGINE_MUTATION_UNVERIFIED")}
  Ok(r["result"].clone())
 }
 fn open(&mut self,path:&Path)->R<u64>{
  let path=path.to_str().ok_or("INVALID_CAD_PATH")?;
  self.call(serde_json::json!({"op":"open","path":path}))?;
  self.call(serde_json::json!({"protocol":1,"op":"state"}))?["document_id"].as_u64().filter(|v|*v>0).ok_or("CAD_ENGINE_DOCUMENT_INVALID")
 }
 fn entities(&mut self,detail:&str)->R<std::collections::BTreeMap<String,Value>>{
  let r=self.call(serde_json::json!({"op":"query","detail":detail,"limit":513}))?;
  let records=r["entities"].as_array().ok_or("CAD_ENGINE_INVALID_RESPONSE")?;
  if records.len()>512||r["count"].as_u64()!=Some(records.len()as u64)||!r["next_offset"].is_null(){return Err("CAD_ENGINE_ENTITY_LIMIT")}
  let mut out=std::collections::BTreeMap::new();
  for v in records{let h=v["handle"].as_str().filter(|h|valid_handle(h)).ok_or("CAD_ENGINE_INVALID_HANDLE")?;
   if out.insert(h.into(),v.clone()).is_some(){return Err("CAD_ENGINE_DUPLICATE_HANDLE")}
  }
  Ok(out)
 }
}
fn supported(v:&Value)->bool{
 v["space"].as_str().is_some_and(|s|s.eq_ignore_ascii_case("*Model_Space"))&&v["type"].as_str().is_some_and(|s|matches!(s.to_ascii_lowercase().as_str(),"point"|"line"|"lwpolyline"|"polyline"|"arc"|"circle"|"hatch"|"insert"))
}
fn stage(store:&ovg_project_store::Store,asset:Uuid,revision:u64)->R<(tempfile::TempDir,PathBuf,ovg_project_store::asset_store::AssetMeta)>{
 if store.revision().map_err(|_|"STORE_QUERY_FAILED")?!=revision{return Err("REVISION_CONFLICT")}
 let a=store.assets().map_err(|_|"STORE_QUERY_FAILED")?.into_iter().find(|v|v.id==asset).ok_or("STALE_ASSET")?;
 let fmt=match a.kind.as_str(){"cad-native-dxf"=>"dxf","cad-native-dwg"=>"dwg",_=>return Err("CAD_FORMAT_UNSUPPORTED")};
 if a.bytes>MAX{return Err("FILE_LIMIT")}
 let dir=tempfile::tempdir().map_err(|_|"STORAGE_UNAVAILABLE")?;let path=dir.path().join(format!("input.{fmt}"));
 store.materialize_asset(asset,&path,&AtomicBool::new(false)).map_err(|_|"CAD_ASSET_READ_FAILED")?;drawing(&path,fmt)?;Ok((dir,path,a))
}
fn transform(exe:&Path,dir:&Path,src:&Path,dst:&Path,handle:&str,edit:EngineEdit)->R<String>{
 if !valid_handle(handle){return Err("CAD_ENGINE_INVALID_HANDLE")}
 let mut request=parameters(edit)?;
 let mut engine=Engine::start(exe,dir)?;let doc=engine.open(src)?;
 let before=engine.entities("full")?;let selected=before.get(handle).filter(|v|supported(v)).ok_or("CAD_ENGINE_ENTITY_UNSUPPORTED")?;
 request["op"]=serde_json::json!("entities_transform");request["handles"]=serde_json::json!([handle]);
 let r=engine.mutate(doc,request)?;
 if r["affected"].as_u64()!=Some(1)||r["created"].as_array().is_some_and(|a|!a.is_empty()){return Err("CAD_ENGINE_TRANSFORM_UNVERIFIED")}
 let after=engine.entities("full")?;
 if before.len()!=after.len(){return Err("CAD_ENGINE_FIDELITY_FAILED")}
 for(h,a)in &before{
  let b=after.get(h).ok_or("CAD_ENGINE_FIDELITY_FAILED")?;
  if h!=handle&&a!=b{return Err("CAD_ENGINE_DEPENDENT_EDIT_REFUSED")}
  if h==handle&&(a==b||a["type"]!=b["type"]||a["layer"]!=b["layer"]||a["owner"]!=b["owner"]){return Err("CAD_ENGINE_FIDELITY_FAILED")}
 }
 if selected["handle"]!=handle{return Err("CAD_ENGINE_FIDELITY_FAILED")}
 let dst_text=dst.to_str().ok_or("INVALID_CAD_PATH")?;
 let result=engine.mutate(doc,serde_json::json!({"op":"save_verified","path":dst_text,"overwrite":false,"allow_lossy":false}))?;
 let sha=result["sha256"].as_str().filter(|s|s.len()==64).ok_or("CAD_ENGINE_SAVE_UNVERIFIED")?;
 if result["verified"]!=true||digest(dst,MAX)?.1!=sha{return Err("CAD_ENGINE_SAVE_UNVERIFIED")}
 drawing(dst,format(dst)?)?;Ok(sha.into())
}
#[tauri::command]
pub async fn native_cad_engine_entities(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,asset:Uuid,expected:String)->Reply<Value>{
 trusted(&window)?;if !cfg!(feature="native-cad"){return Err("CAD_RUNTIME_NOT_PACKAGED".into())}
 let root=window.app_handle().path().resource_dir().map_err(|_|"CAD_RUNTIME_NOT_PACKAGED")?;
 work(window,u.inner().clone(),move|s|{
  let rev=unified::number(&expected)?;let exe=executable(&root)?;
  let(dir,path,a)=s.with(handle,|store|stage(store,asset,rev))?;
  let mut engine=Engine::start(&exe,dir.path())?;engine.open(&path)?;
  let entities:Vec<_>=engine.entities("summary")?.into_values().filter(supported).map(|v|serde_json::json!({"handle":v["handle"],"type":v["type"],"layer":v["layer"]})).collect();
  s.with(handle,|store|if store.revision().map_err(|_|"STORE_QUERY_FAILED")?==rev{Ok(())}else{Err("REVISION_CONFLICT")})?;
  Ok(serde_json::json!({"sha256":a.sha256,"entities":entities}))
 }).await
}
#[tauri::command]
pub async fn native_cad_engine_transform(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,asset:Uuid,expected:String,sha256:String,entity:String,edit:EngineEdit)->Reply<ovg_project_store::asset_store::AssetMeta>{
 trusted(&window)?;if !cfg!(feature="native-cad"){return Err("CAD_RUNTIME_NOT_PACKAGED".into())}
 let root=window.app_handle().path().resource_dir().map_err(|_|"CAD_RUNTIME_NOT_PACKAGED")?;
 work(window,u.inner().clone(),move|s|{
  let rev=unified::number(&expected)?;let exe=executable(&root)?;
  let(dir,src,a)=s.with(handle,|store|stage(store,asset,rev))?;
  if a.sha256!=sha256{return Err("CAD_PARENT_DIGEST_CHANGED")}
  let dst=dir.path().join(format!("edited.{}",format(&src)?));
  let hash=transform(&exe,dir.path(),&src,&dst,&entity,edit)?;
  s.with(handle,|store|store.attach_cad_version(Uuid::new_v4(),&a.kind,&dst,rev,&hash,Some((asset,&a.sha256)),&AtomicBool::new(false)).map_err(|_|"CAD_ADOPTION_FAILED"))
 }).await
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn typed_transform_limits(){
  assert!(parameters(EngineEdit::Move{dx:2.,dy:3.}).is_ok());assert!(parameters(EngineEdit::Move{dx:f64::NAN,dy:0.}).is_err());
  assert!(parameters(EngineEdit::Scale{x:0.,y:0.,factor:0.}).is_err());assert!(parameters(EngineEdit::Rotate{x:0.,y:0.,angle:360.}).is_err());
  assert!(valid_handle("AB12"));assert!(!valid_handle("../file"));assert!(!valid_handle("ab"));
 }
 #[test]
 #[ignore = "requires exact packaged OpenVietCAD executable"]
 fn packaged_engine_roundtrip(){
  let exe=PathBuf::from(std::env::var_os("OVG_CAD_ENGINE_BINARY").expect("packaged engine path")).canonicalize().unwrap();
  let dir=tempfile::tempdir().unwrap();let input=dir.path().join("fixture.dxf");let output=dir.path().join("edited.dxf");
  let mut e=Engine::start(&exe,dir.path()).unwrap();
  e.call(serde_json::json!({"op":"new"})).unwrap();
  let doc=e.call(serde_json::json!({"protocol":1,"op":"state"})).unwrap()["document_id"].as_u64().unwrap();
  let created=e.mutate(doc,serde_json::json!({"op":"entities_create","entities":[{"type":"Line","start":[0,0],"end":[10,0]},{"type":"Circle","center":[30,30],"radius":3}]})).unwrap();
  let h=created["handles"][0].as_str().unwrap().to_owned();
  let r=e.mutate(doc,serde_json::json!({"op":"save_verified","path":input.to_str().unwrap(),"target_version":"2013","overwrite":false,"allow_lossy":false})).unwrap();
  assert_eq!(r["verified"],true);drop(e);
  let sha=transform(&exe,dir.path(),&input,&output,&h,EngineEdit::Move{dx:5.,dy:2.}).unwrap();assert_eq!(digest(&output,MAX).unwrap().1,sha);
  let mut e=Engine::start(&exe,dir.path()).unwrap();e.open(&output).unwrap();let entities=e.entities("full").unwrap();
  assert_eq!(entities[&h]["start"],serde_json::json!([5.,2.,0.]));assert_eq!(entities.len(),2);
 }

}
