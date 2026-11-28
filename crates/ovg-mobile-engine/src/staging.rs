//! Host-only archive extraction to a new private directory. No links, no overwrite.
use std::{fs::{self,File,OpenOptions},io::{Read,Write},path::{Path,Component},sync::atomic::{AtomicBool,Ordering},collections::HashSet};
use crate::Error;
pub fn unpack_zip(input:&Path,destination:&Path,cancel:&AtomicBool,max_bytes:u64)->Result<(),Error>{
    if max_bytes==0||max_bytes>512*1024*1024{return Err(Error::Limit);}
    let f=File::open(input).map_err(|_|Error::InvalidBatch)?;
    if f.metadata().map_err(|_|Error::InvalidBatch)?.len()>max_bytes{return Err(Error::Limit);}
    let mut zip=zip::ZipArchive::new(f).map_err(|_|Error::InvalidBatch)?;
    if zip.len()>512{return Err(Error::Limit);}
    fs::create_dir(destination).map_err(|_|Error::InvalidBatch)?;
    let result=(||{
        let mut total=0u64;let mut seen=HashSet::new();
        for i in 0..zip.len(){
            if cancel.load(Ordering::Acquire){return Err(Error::Cancelled);}
            let mut entry=zip.by_index(i).map_err(|_|Error::InvalidBatch)?;
            if entry.name().contains('\\')||entry.name().contains(':')||entry.name().contains('\0'){return Err(Error::InvalidBatch);}
            let path=entry.enclosed_name().ok_or(Error::InvalidBatch)?;
            if path.components().count()>12||path.as_os_str().len()>240||path.components().any(|c|!matches!(c,Component::Normal(_))){return Err(Error::InvalidBatch);}
            if !seen.insert(path.to_string_lossy().to_lowercase()){return Err(Error::InvalidBatch);}
            if entry.unix_mode().is_some_and(|m|{let kind=m&0o170000;kind!=0&&kind!=0o100000&&kind!=0o040000}){return Err(Error::InvalidBatch);}
            let out=destination.join(path);
            if entry.is_dir(){fs::create_dir_all(&out).map_err(|_|Error::InvalidBatch)?;continue;}
            let expected=entry.size();
            if expected>max_bytes-total||expected>entry.compressed_size().saturating_mul(200).saturating_add(1024*1024){return Err(Error::Limit);}
            fs::create_dir_all(out.parent().ok_or(Error::InvalidBatch)?).map_err(|_|Error::InvalidBatch)?;
            let mut file=OpenOptions::new().write(true).create_new(true).open(&out).map_err(|_|Error::InvalidBatch)?;
            let mut actual=0u64;let mut buf=[0u8;65536];
            loop{if cancel.load(Ordering::Acquire){return Err(Error::Cancelled);}
                let n=entry.read(&mut buf).map_err(|_|Error::InvalidBatch)?;
                if n==0{break;}actual+=n as u64;total+=n as u64;
                if actual>expected||total>max_bytes{return Err(Error::Limit);}
                file.write_all(&buf[..n]).map_err(|_|Error::InvalidBatch)?;
            }
            if actual!=expected{return Err(Error::InvalidBatch);}
            file.sync_all().map_err(|_|Error::InvalidBatch)?;
        }
        Ok(())
    })();
    if result.is_err(){let _=fs::remove_dir_all(destination);}
    result
}
/// Require same-stem mandatory companion files; case-insensitive file names only.
pub fn companions(path:&Path)->Result<Vec<std::path::PathBuf>,Error>{
    let ext=path.extension().and_then(|s|s.to_str()).ok_or(Error::InvalidBatch)?.to_ascii_lowercase();
    let required:&[&str]=match ext.as_str(){"shp"=>&["shp","shx","dbf","prj"],"tab"=>&["tab","dat","map","id"],"mif"=>&["mif","mid"],_=>return Err(Error::InvalidBatch)};
    let stem=path.file_stem().and_then(|s|s.to_str()).ok_or(Error::InvalidBatch)?;
    let mut files=Vec::new();
    for item in fs::read_dir(path.parent().ok_or(Error::InvalidBatch)?).map_err(|_|Error::InvalidBatch)?{
        let item=item.map_err(|_|Error::InvalidBatch)?;let p=item.path();
        if item.file_type().map_err(|_|Error::InvalidBatch)?.is_file()&&p.file_stem().and_then(|s|s.to_str()).is_some_and(|s|s.eq_ignore_ascii_case(stem)){files.push(p);}
        if files.len()>64{return Err(Error::Limit);}
    }
    for ext in required {if files.iter().filter(|p|p.extension().and_then(|s|s.to_str()).is_some_and(|s|s.eq_ignore_ascii_case(ext))).count()!=1{return Err(Error::InvalidBatch);}}
    Ok(files)
}
