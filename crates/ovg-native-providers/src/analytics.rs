//! DuckDB in-memory bounded analytics. No caller SQL or arbitrary extension path.
use crate::Result;
use duckdb::{Config,Connection,params};
use serde::Serialize;
#[derive(Serialize)]
pub struct Summary{pub count:u64,pub min_x:Option<f64>,pub min_y:Option<f64>,pub max_x:Option<f64>,pub max_y:Option<f64>,pub spatial:bool}
/// Only signed locally installed spatial extension, with auto-install/autoload disabled.
/// Once loaded, disable external access before any query or supplied data.
fn connection(spatial:bool)->Result<Connection>{
 let config=Config::default().enable_external_access(spatial).map_err(|_|"DUCKDB_CONFIG")?.enable_autoload_extension(false).map_err(|_|"DUCKDB_CONFIG")?
  .max_memory("128MB").map_err(|_|"DUCKDB_CONFIG")?.threads(2).map_err(|_|"DUCKDB_CONFIG")?
  .with("allow_unsigned_extensions","false").map_err(|_|"DUCKDB_CONFIG")?
  .with("max_temp_directory_size","0B").map_err(|_|"DUCKDB_CONFIG")?;
 let c=Connection::open_in_memory_with_flags(config).map_err(|_|"DUCKDB_OPEN")?;
 if spatial{c.execute_batch("LOAD spatial").map_err(|_|"SIGNED_LOCAL_SPATIAL_EXTENSION_REQUIRED")?;}
 c.execute_batch("SET enable_external_access=false; SET lock_configuration=true; CREATE TABLE points(x DOUBLE NOT NULL,y DOUBLE NOT NULL)").map_err(|_|"DUCKDB_CONFIG")?;
 Ok(c)
}
pub fn summarize(points:&[[f64;2]],spatial:bool)->Result<Summary>{
 if points.len()>100_000||points.iter().flatten().any(|n|!n.is_finite()){return Err("INVALID_POINTS")}
 let mut c=connection(spatial)?;
 {let tx=c.transaction().map_err(|_|"DUCKDB_WRITE")?;
  {let mut q=tx.prepare("INSERT INTO points VALUES (?,?)").map_err(|_|"DUCKDB_WRITE")?;
   for p in points{q.execute(params![p[0],p[1]]).map_err(|_|"DUCKDB_WRITE")?;}}
  tx.commit().map_err(|_|"DUCKDB_WRITE")?;
 }
 let sql=if spatial{"SELECT count(*),min(ST_X(ST_Point(x,y))),min(ST_Y(ST_Point(x,y))),max(ST_X(ST_Point(x,y))),max(ST_Y(ST_Point(x,y))) FROM points"}else{"SELECT count(*),min(x),min(y),max(x),max(y) FROM points"};
 c.query_row(sql,[],|r|Ok(Summary{count:r.get(0)?,min_x:r.get(1)?,min_y:r.get(2)?,max_x:r.get(3)?,max_y:r.get(4)?,spatial})).map_err(|_|"DUCKDB_QUERY")
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn bounded_summary(){let s=summarize(&[[1.,2.],[3.,4.]],false).unwrap();assert_eq!(s.count,2);assert_eq!(s.max_y,Some(4.));assert!(summarize(&[[f64::NAN,0.]],false).is_err());}
}
