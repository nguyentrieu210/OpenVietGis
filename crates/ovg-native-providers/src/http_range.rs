//! Opt-in public HTTPS byte-range source. No redirects, proxies, credentials,
//! private IPs, or network fallback to full-file download.
use std::{io::{self,Read,Seek,SeekFrom},net::{Ipv4Addr,SocketAddr,ToSocketAddrs},time::{Duration,Instant},collections::VecDeque,sync::{Arc,atomic::{AtomicBool,Ordering}}};
use reqwest::{blocking::{Client,Response},header::{RANGE,IF_MATCH,ETAG,CONTENT_RANGE,CONTENT_LENGTH,CONTENT_ENCODING,ACCEPT_ENCODING},redirect::Policy,Url};
const BLOCK:u64=1024*1024;
const MAX_LENGTH:u64=128*1024*1024*1024;
const MAX_TRANSFER:u64=64*1024*1024;
const MAX_REQUESTS:u32=128;
fn error(s:&str)->io::Error{io::Error::new(io::ErrorKind::Other,s)}
fn public_v4(a:Ipv4Addr)->bool{
 let [a,b,c,_]=a.octets();
 !(a==0||a==10||a==127||a>=224||a==169&&b==254||a==172&&(16..=31).contains(&b)||a==192&&(b==168||b==0||b==88&&c==99)||a==100&&(64..=127).contains(&b)||a==198&&(b==18||b==19||b==51&&c==100)||a==203&&b==0&&c==113)
}
fn endpoint(raw:&str)->io::Result<(Url,String)>{
 if raw.len()>2048{return Err(error("URL_LIMIT"))}
 let u=Url::parse(raw).map_err(|_|error("INVALID_URL"))?;
 let host=u.host_str().ok_or_else(||error("INVALID_HOST"))?.to_owned();
 if u.scheme()!="https"||!u.username().is_empty()||u.password().is_some()||u.port_or_known_default()!=Some(443)||u.query().is_some()||u.fragment().is_some()||!host.contains('.')||host.ends_with('.')||host.parse::<std::net::IpAddr>().is_ok()||host.ends_with(".local")||host.ends_with(".internal"){return Err(error("PUBLIC_HTTPS_REQUIRED"))}
 Ok((u,host))
}
fn content_range(s:&str)->io::Result<(u64,u64,u64)>{
 let (span,total)=s.strip_prefix("bytes ").and_then(|x|x.split_once('/')).ok_or_else(||error("INVALID_CONTENT_RANGE"))?;
 let (a,b)=span.split_once('-').ok_or_else(||error("INVALID_CONTENT_RANGE"))?;
 let n=|v:&str|v.parse::<u64>().map_err(|_|error("INVALID_CONTENT_RANGE"));
 let (a,b,t)=(n(a)?,n(b)?,n(total)?);
 if a>b||b>=t||t>MAX_LENGTH{return Err(error("INVALID_CONTENT_RANGE"))}Ok((a,b,t))
}
fn etag(r:&Response)->io::Result<String>{
 let s=r.headers().get(ETAG).and_then(|s|s.to_str().ok()).ok_or_else(||error("STRONG_ETAG_REQUIRED"))?;
 if s.len()<2||s.len()>256||!s.starts_with('"')||!s.ends_with('"')||s[1..s.len()-1].bytes().any(|b|b<=32||b==34||b>=127){return Err(error("STRONG_ETAG_REQUIRED"))}Ok(s.to_owned())
}
fn read_response(mut r:Response,a:u64,b:u64,identity:Option<(&str,u64)>)->io::Result<(Vec<u8>,String,u64)>{
 if r.status()!=reqwest::StatusCode::PARTIAL_CONTENT{return Err(error("HTTP_RANGE_REQUIRED"))}
 if r.headers().get(CONTENT_ENCODING).is_some_and(|h|h.as_bytes()!=b"identity"){return Err(error("ENCODED_RANGE_REJECTED"))}
 let (start,end,total)=content_range(r.headers().get(CONTENT_RANGE).and_then(|h|h.to_str().ok()).ok_or_else(||error("CONTENT_RANGE_REQUIRED"))?)?;
 let tag=etag(&r)?;
 if start!=a||end!=b||identity.is_some_and(|(e,n)|e!=tag||n!=total){return Err(error("REMOTE_SOURCE_CHANGED"))}
 let len=b-a+1;
 if len>BLOCK||r.headers().get(CONTENT_LENGTH).and_then(|h|h.to_str().ok()).and_then(|s|s.parse::<u64>().ok())!=Some(len){return Err(error("CONTENT_LENGTH_REQUIRED"))}
 let mut out=Vec::with_capacity(len as usize);r.take(len+1).read_to_end(&mut out)?;
 if out.len()as u64!=len{return Err(error("TRUNCATED_RANGE"))}
 Ok((out,tag,total))
}
#[derive(Clone,Debug,serde::Serialize)]
pub struct Identity{pub bytes:u64,pub etag:String}
pub struct RangeSource{
 client:Client,url:Url,identity:Identity,pos:u64,cache:VecDeque<(u64,Vec<u8>)>,transfer:u64,requests:u32,deadline:Instant,cancel:Arc<AtomicBool>
}
impl RangeSource{
 pub fn open(raw:&str,cancel:Arc<AtomicBool>)->io::Result<Self>{
  if cancel.load(Ordering::Acquire){return Err(error("CANCELLED"))}
  let (url,host)=endpoint(raw)?;
  let addresses:Vec<_>=(host.as_str(),443).to_socket_addrs()?.take(17).collect();
  if addresses.is_empty()||addresses.len()>16||addresses.iter().any(|s|match s{SocketAddr::V4(a)=>!public_v4(*a.ip()),SocketAddr::V6(_)=>true}){return Err(error("PUBLIC_IPV4_ONLY"))}
  let client=Client::builder().redirect(Policy::none()).no_proxy().connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(12)).resolve_to_addrs(&host,&addresses).build().map_err(|_|error("HTTP_CLIENT_FAILED"))?;
  let r=client.get(url.clone()).header(RANGE,"bytes=0-0").header(ACCEPT_ENCODING,"identity").send().map_err(|_|error("HTTP_RANGE_FAILED"))?;
  let (_,etag,bytes)=read_response(r,0,0,None)?;
  Ok(Self{client,url,identity:Identity{bytes,etag},pos:0,cache:VecDeque::new(),transfer:1,requests:1,deadline:Instant::now()+Duration::from_secs(90),cancel})
 }
 pub fn identity(&self)->&Identity{&self.identity}
 pub fn require_identity(&self,id:&Identity)->io::Result<()>{
  if self.identity.bytes!=id.bytes||self.identity.etag!=id.etag{return Err(error("REMOTE_SOURCE_CHANGED"))}Ok(())
 }
 fn check(&self)->io::Result<()>{
  if self.cancel.load(Ordering::Acquire){return Err(error("CANCELLED"))}
  if Instant::now()>self.deadline{return Err(error("REMOTE_DEADLINE"))}Ok(())
 }
 fn block(&mut self,start:u64)->io::Result<&[u8]>{
  self.check()?;
  if let Some(i)=self.cache.iter().position(|(k,_)|*k==start){
   let b=self.cache.remove(i).ok_or_else(||error("CACHE_FAILED"))?;self.cache.push_back(b);
  }else{
   let end=(start+BLOCK).min(self.identity.bytes)-1;let len=end-start+1;
   if self.requests>=MAX_REQUESTS||self.transfer.saturating_add(len)>MAX_TRANSFER{return Err(error("REMOTE_BUDGET"))}
   self.requests+=1;self.transfer+=len;
   let r=self.client.get(self.url.clone()).header(RANGE,format!("bytes={start}-{end}")).header(IF_MATCH,&self.identity.etag).header(ACCEPT_ENCODING,"identity").send().map_err(|_|error("HTTP_RANGE_FAILED"))?;
   let (bytes,_,_)=read_response(r,start,end,Some((&self.identity.etag,self.identity.bytes)))?;
   if self.cache.len()>=8{self.cache.pop_front();}self.cache.push_back((start,bytes));
  }
  Ok(&self.cache.back().ok_or_else(||error("CACHE_FAILED"))?.1)
 }
}
impl Read for RangeSource{
 fn read(&mut self,out:&mut [u8])->io::Result<usize>{
  self.check()?;if out.is_empty()||self.pos==self.identity.bytes{return Ok(0)}
  let start=self.pos/BLOCK*BLOCK;let off=(self.pos-start)as usize;
  let b=self.block(start)?;let n=out.len().min(b.len()-off);out[..n].copy_from_slice(&b[off..off+n]);self.pos+=n as u64;Ok(n)
 }
}
impl Seek for RangeSource{
 fn seek(&mut self,p:SeekFrom)->io::Result<u64>{
  self.check()?;
  let pos=match p{SeekFrom::Start(n)=>i128::from(n),SeekFrom::Current(n)=>i128::from(self.pos)+i128::from(n),SeekFrom::End(n)=>i128::from(self.identity.bytes)+i128::from(n)};
  if pos<0||pos>i128::from(self.identity.bytes){return Err(error("INVALID_REMOTE_SEEK"))}
  self.pos=pos as u64;Ok(self.pos)
 }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn rejects_unsafe_endpoints(){
  for s in ["http://example.com/a","https://127.0.0.1/a","https://user@example.com/a","https://example.com:8443/a","https://example.com/a?secret=1","https://x.local/a","https://example.com/a#x"]{assert!(endpoint(s).is_err(),"{s}")}
  assert!(endpoint("https://example.com/cloud.laz").is_ok());
  for s in ["127.0.0.1","169.254.169.254","10.0.0.1","192.168.0.1","100.64.0.1","198.18.0.1","203.0.113.1","224.0.0.1"]{assert!(!public_v4(s.parse().unwrap()))}
  assert!(public_v4("8.8.8.8".parse().unwrap()));
 }
 #[test]fn validates_content_range(){
  assert_eq!(content_range("bytes 0-0/123").unwrap(),(0,0,123));
  for s in ["bytes 2-1/4","bytes 0-4/4","bytes 0-0/*","bytes 0-0/0","bytes 0-0/999999999999999"]{assert!(content_range(s).is_err())}
 }
}
