//! Experimental Windows CPU sparse worker with suspended launch and Job Object.
//! Job Objects provide resource/lifetime containment, NOT filesystem/network isolation.
//! Only trusted host qualification/testing may invoke this path; no Desktop IPC.
use super::{colmap,local_worker::{self,ApprovedPack,Limits},Device};
use std::path::Path;
#[derive(Debug)]
pub enum Error{Unsupported,Invalid,Limit,Cancelled,Timeout,Failed(u32),Io(std::io::Error),Pack(local_worker::Error)}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
/// Windows CRT-compatible quoting. No command interpreter is invoked.
fn quote(arg:&[u16])->Result<Vec<u16>,Error>{
 if arg.contains(&0){return Err(Error::Invalid);}
 let mut out=vec![34];let mut slash=0usize;
 for &ch in arg{
 if ch==92{slash+=1;continue;}
 let n=if ch==34{slash*2+1}else{slash};
 out.extend(std::iter::repeat(92).take(n));slash=0;out.push(ch);
 }
 out.extend(std::iter::repeat(92).take(slash*2));out.push(34);Ok(out)
}
/// CPU-only, fixed internal commands. No untrusted command strings or executable paths.
/// Caller must ensure a dedicated trusted test account, immutable pack and input,
/// offline ACL/firewall containment, disk quota and independent output validation.
pub fn sparse_candidate(root:&Path,pack:&ApprovedPack,config:&colmap::Config,limits:Limits,
 approved_manifest:&str,max_processes:u32,mut cancelled:impl FnMut()->bool)->Result<(),Error>{
 if config.device!=Device::Cpu||config.gpu_index.is_some(){return Err(Error::Unsupported);}
 if !root.is_absolute()||!local_worker::valid_limits(limits)||max_processes==0||max_processes>32{return Err(Error::Invalid);}
 #[cfg(not(windows))]{let _=(pack,approved_manifest,&mut cancelled);Err(Error::Unsupported)}
 #[cfg(windows)]{
 use std::{fs,time::Instant};
 let binary=local_worker::verify(pack).map_err(Error::Pack)?;let root=root.canonicalize()?;
 if binary.starts_with(&root)||root.parent().is_none()||root.join("database.db").exists(){return Err(Error::Invalid);}
 for n in ["images","sparse"]{let m=fs::symlink_metadata(root.join(n))?;if !m.file_type().is_dir(){return Err(Error::Invalid);}}
 if fs::read_dir(root.join("sparse"))?.next().is_some(){return Err(Error::Invalid);}
 let hash=super::ingest::verify_staged(&root,config.image_count,limits.input_bytes,&mut cancelled).map_err(|_|Error::Invalid)?;
 if hash!=approved_manifest{return Err(Error::Invalid);}
 let mut count=0u32;
 for e in fs::read_dir(root.join("images"))?{let e=e?;if !e.file_type()?.is_file(){return Err(Error::Invalid);}
 count=count.checked_add(1).ok_or(Error::Limit)?;if count>config.image_count{return Err(Error::Invalid);}}
 if count!=config.image_count{return Err(Error::Invalid);}
 let layout=colmap::Layout::from_host_root(root.clone()).map_err(|_|Error::Invalid)?;
 let plan=colmap::sparse_plan(&layout,config).map_err(|_|Error::Invalid)?;
 let start=Instant::now();
 for s in plan{
 let remaining=limits.timeout.checked_sub(start.elapsed()).ok_or(Error::Timeout)?;
 if remaining.is_zero(){return Err(Error::Timeout);}
 run_step(&root,&binary,&s,limits.address_space_bytes,max_processes,remaining,&mut cancelled)?;
 }
 let found=fs::read_dir(root.join("sparse"))?.filter_map(Result::ok).any(|e|{
 e.file_type().is_ok_and(|t|t.is_dir())&&["cameras.bin","images.bin","points3D.bin"].iter().all(|n|{
 fs::symlink_metadata(e.path().join(n)).is_ok_and(|m|m.file_type().is_file()&&m.len()>0&&m.len()<=limits.max_file_bytes)
 })});
 if !found{return Err(Error::Invalid);}Ok(())
 }
}
#[cfg(windows)]
fn run_step(root:&Path,binary:&Path,step:&colmap::Step,memory:u64,max_processes:u32,timeout:std::time::Duration,
 cancelled:&mut impl FnMut()->bool)->Result<(),Error>{
 use std::{os::windows::ffi::OsStrExt,ptr::{null,null_mut},time::Instant};
 use windows_sys::Win32::{Foundation::{CloseHandle,HANDLE,WAIT_OBJECT_0,WAIT_TIMEOUT},
 System::{JobObjects::{CreateJobObjectW,SetInformationJobObject,AssignProcessToJobObject,TerminateJobObject,
 JOBOBJECT_EXTENDED_LIMIT_INFORMATION,JobObjectExtendedLimitInformation,JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
 JOB_OBJECT_LIMIT_JOB_MEMORY,JOB_OBJECT_LIMIT_ACTIVE_PROCESS},
 Threading::{CreateProcessW,ResumeThread,TerminateProcess,WaitForSingleObject,GetExitCodeProcess,
 PROCESS_INFORMATION,STARTUPINFOW,CREATE_SUSPENDED,CREATE_NO_WINDOW,CREATE_UNICODE_ENVIRONMENT}}};
 struct Owned(HANDLE);impl Drop for Owned{fn drop(&mut self){if !self.0.is_null(){unsafe{CloseHandle(self.0);}}}}
 fn err()->Error{Error::Io(std::io::Error::last_os_error())}
 if cancelled(){return Err(Error::Cancelled);}
 let mem=usize::try_from(memory).map_err(|_|Error::Limit)?;
 let mut command=quote(&binary.as_os_str().encode_wide().collect::<Vec<_>>())?;
 for a in &step.args{command.push(32);command.extend(quote(&a.encode_wide().collect::<Vec<_>>())?);}
 if command.len()>32766{return Err(Error::Limit);}command.push(0);
 let wide=|p:&Path|->Result<Vec<u16>,Error>{let mut v:Vec<_>=p.as_os_str().encode_wide().collect();if v.contains(&0){return Err(Error::Invalid);}v.push(0);Ok(v)};
 let executable=wide(binary)?;let cwd=wide(root)?;
 // Minimal explicit environment. Host must separately qualify DLL resolution.
 let system=std::env::var_os("SystemRoot").ok_or(Error::Invalid)?;
 let system_path=std::path::PathBuf::from(&system);if !system_path.is_absolute(){return Err(Error::Invalid);}
 let temp=root.join("worker-temp");std::fs::create_dir_all(&temp)?;
 if !std::fs::symlink_metadata(&temp)?.file_type().is_dir(){return Err(Error::Invalid);}
 let mut env=Vec::<u16>::new();
 for (key,value) in [("SystemRoot",system),("TEMP",temp.as_os_str().to_owned()),("TMP",temp.as_os_str().to_owned())]{
 env.extend(key.encode_utf16());env.push(61);let v:Vec<_>=value.encode_wide().collect();if v.contains(&0){return Err(Error::Invalid);}env.extend(v);env.push(0);
 }env.push(0);
 // SAFETY: valid initialized Win32 structures, owned handles, all UTF-16 buffers
 // stay alive through CreateProcessW; process starts suspended before assignment.
 unsafe{
 let job=Owned(CreateJobObjectW(null(),null()));if job.0.is_null(){return Err(err());}
 let mut info:JOBOBJECT_EXTENDED_LIMIT_INFORMATION=std::mem::zeroed();
 info.BasicLimitInformation.LimitFlags=JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE|JOB_OBJECT_LIMIT_JOB_MEMORY|JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
 info.BasicLimitInformation.ActiveProcessLimit=max_processes;info.JobMemoryLimit=mem;
 if SetInformationJobObject(job.0,JobObjectExtendedLimitInformation,(&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),std::mem::size_of_val(&info) as u32)==0{return Err(err());}
 let mut startup:STARTUPINFOW=std::mem::zeroed();startup.cb=std::mem::size_of_val(&startup) as u32;
 let mut pi:PROCESS_INFORMATION=std::mem::zeroed();
 if CreateProcessW(executable.as_ptr(),command.as_mut_ptr(),null(),null(),0,
 CREATE_SUSPENDED|CREATE_NO_WINDOW|CREATE_UNICODE_ENVIRONMENT,env.as_ptr().cast(),cwd.as_ptr(),&startup,&mut pi)==0{return Err(err());}
 let process=Owned(pi.hProcess);let thread=Owned(pi.hThread);
 if AssignProcessToJobObject(job.0,process.0)==0{let e=err();TerminateProcess(process.0,1);WaitForSingleObject(process.0,5000);return Err(e);}
 if ResumeThread(thread.0)==u32::MAX{let e=err();TerminateJobObject(job.0,1);return Err(e);}
 let started=Instant::now();
 loop{
 if cancelled()||started.elapsed()>=timeout{
 let cancel=started.elapsed()<timeout;TerminateJobObject(job.0,1);WaitForSingleObject(process.0,5000);
 return Err(if cancel{Error::Cancelled}else{Error::Timeout});}
 match WaitForSingleObject(process.0,25){
 WAIT_OBJECT_0=>{let mut code=0;if GetExitCodeProcess(process.0,&mut code)==0{return Err(err());}
 return if code==0{Ok(())}else{Err(Error::Failed(code))};},
 WAIT_TIMEOUT=>{},_=>return Err(err())
 }
 }
 } // Closing job terminates remaining descendants.
}
#[cfg(test)]mod tests{
 use super::*;
 fn q(s:&str)->String{String::from_utf16(&quote(&s.encode_utf16().collect::<Vec<_>>()).unwrap()).unwrap()}
 #[test]fn quotes_spaces_quotes_and_trailing_slash(){
 assert_eq!(q(""),"\"\"");assert_eq!(q("a b"),"\"a b\"");
 assert_eq!(q("a\"b"),"\"a\\\"b\"");assert_eq!(q("a\\"),"\"a\\\\\"");
 assert!(matches!(quote(&[0]),Err(Error::Invalid)));
 }
}
