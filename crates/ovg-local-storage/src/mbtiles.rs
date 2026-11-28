//! Read-only local MBTiles. This adapter never downloads or serves network data.
use super::Error;
use rusqlite::{Connection,OpenFlags,OptionalExtension,params};
use std::path::Path;
pub struct Archive{db:Connection,pub format:String,pub attribution:String}
impl Archive{
    pub fn open(path:&Path)->Result<Self,Error>{
        if !std::fs::symlink_metadata(path)?.file_type().is_file(){return Err(Error::Invalid);}
        let db=Connection::open_with_flags(path,OpenFlags::SQLITE_OPEN_READ_ONLY|OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        db.pragma_update(None,"trusted_schema","OFF")?;
        db.pragma_update(None,"query_only","ON")?;
        let get=|name:&str|->Result<String,Error>{let mut q=db.prepare("SELECT value FROM metadata WHERE name=?1 AND length(value)<=8192 LIMIT 2")?;let v=q.query_map([name],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;if v.len()!=1{return Err(Error::Invalid);}Ok(v[0].clone())};
        let format=get("format")?;
        if !matches!(format.as_str(),"png"|"jpg"|"jpeg"|"webp"|"pbf"){return Err(Error::Unsupported);}
        let attribution=get("attribution")?;
        if attribution.trim().is_empty(){return Err(Error::Invalid);}
        let scheme=get("scheme").unwrap_or_else(|_|"tms".to_owned());
        if scheme!="tms"{return Err(Error::Unsupported);}
        db.prepare("SELECT zoom_level,tile_column,tile_row,tile_data FROM tiles LIMIT 0")?;
        Ok(Self{db,format,attribution})
    }
    /// XYZ caller coordinates; MBTiles uses TMS Y. Reject oversized blobs before allocating.
    pub fn tile(&self,z:u8,x:u32,y:u32)->Result<Option<Vec<u8>>,Error>{
        if z>24{return Err(Error::Invalid);}
        let n=1u32<<z;if x>=n||y>=n{return Err(Error::Invalid);}
        let tms=n-1-y;
        let len:Option<u64>=self.db.query_row("SELECT length(tile_data) FROM tiles WHERE zoom_level=?1 AND tile_column=?2 AND tile_row=?3",params![z,x,tms],|r|r.get(0)).optional()?;
        if len.is_some_and(|n|n==0||n>4*1024*1024){return Err(Error::Limit);}
        let data:Option<Vec<u8>>=self.db.query_row("SELECT tile_data FROM tiles WHERE zoom_level=?1 AND tile_column=?2 AND tile_row=?3 AND length(tile_data) BETWEEN 1 AND 4194304",params![z,x,tms],|r|r.get(0)).optional()?;
        if len.is_some()&&data.is_none(){return Err(Error::Invalid);}
        Ok(data)
    }
}
