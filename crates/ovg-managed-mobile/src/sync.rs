//! R029 wire shapes, derived from managed.rs. Delta cursors are coalesced:
 //! never assume one change per consecutive cursor; aliases may repeat cursor.
use super::{Error,rev,feature_id};
use serde::{Deserialize,Serialize};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Feature{pub feature_id:String,pub geometry:Value,pub properties:Value,pub content_revision:String}
impl Feature{
 pub fn valid(&self)->bool{feature_id(&self.feature_id)&&rev(&self.content_revision)&&self.properties.is_object()&&(self.geometry.is_object()||self.geometry.is_null())}
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Bootstrap{pub layer_id:Uuid,pub source_id:Uuid,pub content_revision:String,pub schema_revision:String,pub cursor:i64,pub permission_epoch:i64,pub cacheable:bool,pub features:Vec<Feature>,pub has_more:bool,pub next_offset:Option<i64>}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Change{pub cursor:i64,pub layer_id:Uuid,pub source_id:Uuid,pub content_revision:String,pub upserts:Vec<Feature>,pub removals:Vec<String>}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Delta{pub reset_required:bool,pub next_cursor:i64,pub permission_epoch:i64,pub has_more:bool,pub changes:Vec<Change>}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Receipt{pub status:String,pub operation_id:String,pub source_id:Uuid,pub resulting_content_revision:String,pub event_cursor:i64,pub changeset_id:Uuid,pub operation_count:u32}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Commit{pub session_id:Uuid,pub source_id:Uuid,pub base_content_revision:String,pub base_schema_revision:Option<String>,pub operation_id:String,pub allow_deletes:bool,pub operations:Vec<Value>}
impl Commit{
 pub fn validate(&self)->Result<(),Error>{
  if self.session_id.is_nil()||self.source_id.is_nil()||!rev(&self.base_content_revision)||self.base_schema_revision.as_ref().is_none_or(|s|!rev(s))||!feature_id(&self.operation_id)||self.operation_id.starts_with("field-sync-")||self.operations.is_empty()||self.operations.len()>1000{return Err(Error::Invalid);}
  for op in &self.operations{if !op.is_object()||!op["feature_id"].as_str().is_some_and(feature_id)||!matches!(op["op_type"].as_str(),Some("insert"|"update"|"delete"))||(op["op_type"]=="delete"&&!self.allow_deletes){return Err(Error::Invalid);}}
  Ok(())
 }
}
