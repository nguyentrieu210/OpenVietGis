use ovg_local_storage::media::{Vault,qr_text};
use zeroize::Zeroizing;
use uuid::Uuid;
#[test]
fn blob_vault_scope_and_tamper(){
 let dir=std::env::temp_dir().join(format!("ovg-vault-{}",Uuid::new_v4()));let scope=Uuid::new_v4();
 let v=Vault::open(&dir,scope,Zeroizing::new([9;32])).unwrap();
 let r=v.put(b"private-survey-photo-fixture").unwrap();assert_eq!(&**v.read(&r).unwrap(),b"private-survey-photo-fixture");
 let p=dir.join(format!("{}.enc",r.id));let mut bytes=std::fs::read(&p).unwrap();
 assert!(!bytes.windows(28).any(|w|w==b"private-survey-photo-fixture"));
 let other=Vault::open(&dir,Uuid::new_v4(),Zeroizing::new([9;32])).unwrap();assert!(other.read(&r).is_err());
 let wrong=Vault::open(&dir,scope,Zeroizing::new([8;32])).unwrap();assert!(wrong.read(&r).is_err());
 bytes[40]^=1;std::fs::write(&p,&bytes).unwrap();assert!(v.read(&r).is_err());
 let staged=dir.join(format!(".{}.staging",Uuid::new_v4()));std::fs::write(&staged,b"encrypted").unwrap();
 assert_eq!(v.cleanup_staging().unwrap(),1);assert!(!staged.exists());assert!(p.exists());
 assert_eq!(qr_text("https://example.invalid").unwrap(),"https://example.invalid");assert!(qr_text("x\ny").is_err());
 std::fs::remove_dir_all(dir).unwrap();
}
