//! Native-only scoped key provider. No key material in Dart/FRB.
use ovg_project_store::{Error,media::MediaKeys};
use uuid::Uuid;
use zeroize::Zeroizing;
pub struct PlatformKeys;
impl MediaKeys for PlatformKeys{
 fn key_for(&self,scope:Uuid)->Result<Zeroizing<[u8;32]>,Error>{platform::get(scope,false)}
}
impl PlatformKeys{
 pub fn provision(scope:Uuid)->Result<(),Error>{platform::get(scope,true).map(drop)}
 pub fn available()->bool{platform::available()}
 pub fn capture_preview()->Result<Zeroizing<Vec<u8>>,Error>{platform::capture_preview()}
 pub fn show_preview(raw:&[u8])->Result<bool,Error>{platform::show_preview(raw)}
 pub fn camera_available()->bool{cfg!(target_os="android")&&Self::available()}
}
#[cfg(target_os="android")]
mod platform{
 use super::*;
 use jni::{JavaVM,JNIEnv,objects::{GlobalRef,JObject,JValue,JByteArray}};
 use std::sync::OnceLock;
 struct Provider{vm:JavaVM,object:GlobalRef}
 static PROVIDER:OnceLock<Provider>=OnceLock::new();
 // Invoked by a native Kotlin instance during Activity initialization.
 #[no_mangle]
 pub extern "system" fn Java_org_openvietgis_secure_MediaKeys_nativeInstall(env:JNIEnv<'_>,obj:JObject<'_>)->jni::sys::jboolean{
  if PROVIDER.get().is_some(){return 1}
  let result=(||->jni::errors::Result<Provider>{Ok(Provider{vm:env.get_java_vm()?,object:env.new_global_ref(obj)?})})();
  match result{Ok(p)=>{let _=PROVIDER.set(p);1},Err(_)=>0}
 }
 pub fn available()->bool{PROVIDER.get().is_some()}
 pub fn capture_preview()->Result<Zeroizing<Vec<u8>>,Error>{
  let p=PROVIDER.get().ok_or(Error::Unsupported)?;
  let mut e=p.vm.attach_current_thread().map_err(|_|Error::Storage)?;
  let r=e.with_local_frame(4,|env|{
   let value=env.call_method(p.object.as_obj(),"capturePreview","()[B",&[])?.l()?;
   if value.is_null(){return Ok::<_,jni::errors::Error>(None)}
   let a=JByteArray::from(value);let n=env.get_array_length(&a)? as usize;
   if n==0||n>16*1024*1024{return Ok(None)}
   let mut signed=Zeroizing::new(vec![0i8;n]);let read=env.get_byte_array_region(&a,0,&mut signed[..]);
   let wipe=env.set_byte_array_region(&a,0,&vec![0i8;n]);read?;wipe?;
   Ok(Some(Zeroizing::new(signed.iter().map(|&v|v as u8).collect::<Vec<_>>())))
  });
  if e.exception_check().unwrap_or(false){let _=e.exception_clear();return Err(Error::Storage)}
  r.map_err(|_|Error::Storage)?.ok_or(Error::Conflict)
 }
 pub fn show_preview(raw:&[u8])->Result<bool,Error>{
  if raw.is_empty()||raw.len()>16*1024*1024{return Err(Error::Limit)}
  let p=PROVIDER.get().ok_or(Error::Unsupported)?;
  let mut e=p.vm.attach_current_thread().map_err(|_|Error::Storage)?;
  let r=e.with_local_frame(4,|env|{
   let a=env.byte_array_from_slice(raw)?;
   let result=env.call_method(p.object.as_obj(),"showPreview","([B)Z",&[JValue::Object(a.as_ref())]);
   let wipe=env.set_byte_array_region(&a,0,&vec![0i8;raw.len()]);
   let shown=result?.z()?;wipe?;Ok::<_,jni::errors::Error>(shown)
  });
  if e.exception_check().unwrap_or(false){let _=e.exception_clear();return Err(Error::Storage)}
  r.map_err(|_|Error::Storage)
 }
 pub fn get(scope:Uuid,create:bool)->Result<Zeroizing<[u8;32]>,Error>{
  if scope.is_nil(){return Err(Error::Invalid)}
  let p=PROVIDER.get().ok_or(Error::Unsupported)?;
  let mut e=p.vm.attach_current_thread().map_err(|_|Error::Storage)?;
  let result=e.with_local_frame(8,|env|{
   let name=env.new_string(scope.to_string())?;
   let value=env.call_method(p.object.as_obj(),"getScopedKey","(Ljava/lang/String;Z)[B",&[JValue::Object(name.as_ref()),JValue::Bool(create as u8)])?.l()?;
   if value.is_null(){return Ok::<_,jni::errors::Error>(None)}
   let a=JByteArray::from(value);
   if env.get_array_length(&a)?!=32{return Ok(None)}
   let mut signed=Zeroizing::new([0i8;32]);
   let read=env.get_byte_array_region(&a,0,&mut signed[..]);
   let wipe=env.set_byte_array_region(&a,0,&[0i8;32]);
   read?;wipe?;
   let mut k=Zeroizing::new([0u8;32]);for i in 0..32{k[i]=signed[i]as u8;}
   Ok(Some(k))
  });
  if e.exception_check().unwrap_or(false){let _=e.exception_clear();return Err(Error::Storage)}
  let k=result.map_err(|_|Error::Storage)?.ok_or(Error::Unsupported)?;
  if k.iter().all(|&x|x==0){return Err(Error::Invalid)}Ok(k)
 }
}
#[cfg(target_os="ios")]
mod platform{
 use super::*;
 use std::{ffi::{c_void,c_char},ptr};
 use chacha20poly1305::aead::{OsRng,rand_core::RngCore};
 type Cf=*const c_void;
 const UTF8:u32=0x08000100;
 const SUCCESS:i32=0;
 const NOT_FOUND:i32=-25300;
 const DUPLICATE:i32=-25299;
 #[link(name="CoreFoundation",kind="framework")]
 extern "C"{
  static kCFBooleanTrue:Cf;
  static kCFTypeDictionaryKeyCallBacks:u8;
  static kCFTypeDictionaryValueCallBacks:u8;
  fn CFStringCreateWithCString(a:Cf,s:*const c_char,encoding:u32)->Cf;
  fn CFDictionaryCreate(a:Cf,k:*const Cf,v:*const Cf,n:isize,kcb:*const c_void,vcb:*const c_void)->Cf;
  fn CFDataCreate(a:Cf,b:*const u8,n:isize)->Cf;
  fn CFDataGetLength(d:Cf)->isize;
  fn CFDataGetBytePtr(d:Cf)->*const u8;
  fn CFGetTypeID(o:Cf)->usize;
  fn CFDataGetTypeID()->usize;
  fn CFRelease(o:Cf);
 }
 #[link(name="Security",kind="framework")]
 extern "C"{
  static kSecClass:Cf;static kSecClassGenericPassword:Cf;
  static kSecAttrService:Cf;static kSecAttrAccount:Cf;
  static kSecAttrAccessible:Cf;static kSecAttrAccessibleWhenUnlockedThisDeviceOnly:Cf;
  static kSecReturnData:Cf;static kSecMatchLimit:Cf;static kSecMatchLimitOne:Cf;static kSecValueData:Cf;
  fn SecItemCopyMatching(q:Cf,result:*mut Cf)->i32;
  fn SecItemAdd(q:Cf,result:*mut Cf)->i32;
 }
 struct Owned(Cf);
 impl Owned{fn new(p:Cf)->Result<Self,Error>{if p.is_null(){Err(Error::Storage)}else{Ok(Self(p))}}}
 impl Drop for Owned{fn drop(&mut self){unsafe{CFRelease(self.0)}}}
 fn string(s:&str)->Result<Owned,Error>{let c=std::ffi::CString::new(s).map_err(|_|Error::Invalid)?;Owned::new(unsafe{CFStringCreateWithCString(ptr::null(),c.as_ptr(),UTF8)})}
 fn dict(pairs:&[(Cf,Cf)])->Result<Owned,Error>{
  let (k,v):(Vec<Cf>,Vec<Cf>)=pairs.iter().copied().unzip();
  Owned::new(unsafe{CFDictionaryCreate(ptr::null(),k.as_ptr(),v.as_ptr(),k.len()as isize,ptr::addr_of!(kCFTypeDictionaryKeyCallBacks).cast(),ptr::addr_of!(kCFTypeDictionaryValueCallBacks).cast())})
 }
 fn lookup(base:&[(Cf,Cf)])->Result<Option<Zeroizing<[u8;32]>>,Error>{
  let mut pairs=base.to_vec();unsafe{pairs.extend([(kSecReturnData,kCFBooleanTrue),(kSecMatchLimit,kSecMatchLimitOne)]);}
  let q=dict(&pairs)?;let mut out=ptr::null();let s=unsafe{SecItemCopyMatching(q.0,&mut out)};
  if s==NOT_FOUND{return Ok(None)}if s!=SUCCESS{return Err(Error::Storage)}
  let d=Owned::new(out)?;
  if unsafe{CFGetTypeID(d.0)!=CFDataGetTypeID()||CFDataGetLength(d.0)!=32}{return Err(Error::Invalid)}
  let mut k=Zeroizing::new([0u8;32]);let bytes=unsafe{CFDataGetBytePtr(d.0)};if bytes.is_null(){return Err(Error::Storage)}
  unsafe{ptr::copy_nonoverlapping(bytes,k.as_mut_ptr(),32)};Ok(Some(k))
 }
 pub fn available()->bool{true}
 pub fn capture_preview()->Result<Zeroizing<Vec<u8>>,Error>{Err(Error::Unsupported)}
 pub fn show_preview(_:&[u8])->Result<bool,Error>{Err(Error::Unsupported)}
 pub fn get(scope:Uuid,create:bool)->Result<Zeroizing<[u8;32]>,Error>{
  if scope.is_nil(){return Err(Error::Invalid)}
  let service=string("org.openvietgis.media.v1")?;let account=string(&scope.to_string())?;
  let base=unsafe{[(kSecClass,kSecClassGenericPassword),(kSecAttrService,service.0),(kSecAttrAccount,account.0)]};
  if let Some(k)=lookup(&base)?{return Ok(k)}if !create{return Err(Error::Unsupported)}
  let mut k=Zeroizing::new([0u8;32]);OsRng.fill_bytes(&mut k[..]);
  let data=Owned::new(unsafe{CFDataCreate(ptr::null(),k.as_ptr(),32)})?;
  let mut pairs=base.to_vec();unsafe{pairs.extend([(kSecAttrAccessible,kSecAttrAccessibleWhenUnlockedThisDeviceOnly),(kSecValueData,data.0)]);}
  let q=dict(&pairs)?;match unsafe{SecItemAdd(q.0,ptr::null_mut())}{SUCCESS=>Ok(k),DUPLICATE=>lookup(&base)?.ok_or(Error::Storage),_=>Err(Error::Storage)}
 }
}
#[cfg(not(any(target_os="android",target_os="ios")))]
mod platform{
 use super::*;
 pub fn available()->bool{false}
 pub fn capture_preview()->Result<Zeroizing<Vec<u8>>,Error>{Err(Error::Unsupported)}
 pub fn show_preview(_:&[u8])->Result<bool,Error>{Err(Error::Unsupported)}
 pub fn get(_:Uuid,_:bool)->Result<Zeroizing<[u8;32]>,Error>{Err(Error::Unsupported)}
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn unsupported_host_fails_closed(){
  #[cfg(not(any(target_os="android",target_os="ios")))]{
   assert!(!PlatformKeys::available());assert!(PlatformKeys.key_for(Uuid::new_v4()).is_err());assert!(PlatformKeys::provision(Uuid::new_v4()).is_err());
  }
 }
}
