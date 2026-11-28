//! Desktop adapter; encoding and clipping are shared with Mobile.
use ovg_local_storage::LocalStore;
pub(crate) use ovg_project_store::mvt::bounds;
pub use ovg_project_store::mvt::unified_tile;
type Result<T>=std::result::Result<T,&'static str>;
pub fn tile(store:&LocalStore,z:u8,x:u32,y:u32,cancel:&std::sync::atomic::AtomicBool)->Result<Vec<u8>>{
 use std::sync::atomic::Ordering;
 if cancel.load(Ordering::Acquire){return Err("CANCELLED")}
 let b=bounds(z,x,y)?;
 let rows=store.query(b,0,512+1).map_err(|_|"STORAGE_UNAVAILABLE")?;
 let last=rows.last().map_or(0,|r|r.0);
 if rows.len()>MAX_FEATURES as usize||!store.query(b,last,1).map_err(|_|"STORAGE_UNAVAILABLE")?.is_empty(){return Err("TILE_DENSITY_LIMIT")}
 ovg_project_store::mvt::encode(rows.into_iter().map(|(_,f)|(f.feature_id,f.wkb)),z,x,y,cancel)
}
