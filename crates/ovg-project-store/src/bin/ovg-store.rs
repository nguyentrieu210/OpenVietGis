//! Offline engineering CLI; no user-data activation or renderer access.
use ovg_project_store::{Store,migration::{migrate,Source}};
use serde::Deserialize;
use std::{path::PathBuf,sync::atomic::AtomicBool};
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input{layer:Uuid,name:String,snapshot:PathBuf}
fn execute()->Result<(),String>{
 let args:Vec<_>=std::env::args_os().skip(1).collect();
 let fail=|e:ovg_project_store::Error|format!("{e:?}");
 let cancel=AtomicBool::new(false);
 match args.first().and_then(|s|s.to_str()){
  Some("migrate") if args.len()==3=>{
   let spec=PathBuf::from(&args[1]);
   if std::fs::metadata(&spec).map_err(|e|e.to_string())?.len()>64*1024{return Err("Manifest too large".into())}
   let input:Vec<Input>=serde_json::from_slice(&std::fs::read(spec).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
   let sources:Vec<_>=input.into_iter().map(|i|Source{layer:i.layer,name:i.name,snapshot:i.snapshot}).collect();
   let report=migrate(&sources,&PathBuf::from(&args[2]),&cancel).map_err(fail)?;
   println!("{}",serde_json::to_string_pretty(&report).map_err(|e|e.to_string())?);
  },
  Some("report") if args.len()==2=>{
   let s=Store::reopen(&PathBuf::from(&args[1])).map_err(fail)?;
   println!("{}",serde_json::to_string_pretty(&s.report().map_err(fail)?).map_err(|e|e.to_string())?);
  },
  Some("compact-copy") if args.len()==4=>{
   let s=Store::reopen(&PathBuf::from(&args[1])).map_err(fail)?;
   let rev=args[3].to_str().ok_or("Invalid revision")?.parse().map_err(|_|"Invalid revision")?;
   s.compact_copy(&PathBuf::from(&args[2]),rev).map_err(fail)?;
  },
  // Repair a verified copy only: no in-place repair CLI on original input.
  Some("rebuild-copy") if args.len()==4=>{
   let src=Store::reopen(&PathBuf::from(&args[1])).map_err(fail)?;
   let rev=args[3].to_str().ok_or("Invalid revision")?.parse().map_err(|_|"Invalid revision")?;
   src.rebuilt_copy(&PathBuf::from(&args[2]),rev,&cancel).map_err(fail)?;
  },
  _=>return Err("usage: ovg-store migrate manifest.json NEW.sqlite | report STORE.sqlite | compact-copy STORE.sqlite NEW.sqlite REV | rebuild-copy STORE.sqlite NEW.sqlite REV".into())
 }
 Ok(())
}
fn main(){if let Err(e)=execute(){eprintln!("{e}");std::process::exit(1)}}
