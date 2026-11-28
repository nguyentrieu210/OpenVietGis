//! Closed local attribute schema, no implicit string/number coercion.
use serde::{Serialize,Deserialize};
use serde_json::Value;
use crate::Error;
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Kind{Text,Integer,Number,Boolean}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct Field{pub name:String,pub kind:Kind,pub required:bool,pub max_length:Option<usize>,pub min:Option<f64>,pub max:Option<f64>}
pub fn validate_schema(fields:&[Field])->Result<(),Error>{
    if fields.len()>128{return Err(Error::Limit);}
    let mut names=std::collections::HashSet::new();
    for f in fields{
        if f.name.is_empty()||f.name.len()>128||f.name.chars().any(char::is_control)||!names.insert(&f.name)||f.max_length.is_some_and(|n|n>65536)||f.min.is_some_and(|n|!n.is_finite())||f.max.is_some_and(|n|!n.is_finite())||matches!((f.min,f.max),(Some(a),Some(b)) if a>b){return Err(Error::InvalidBatch);}
        if (!matches!(f.kind,Kind::Text)&&f.max_length.is_some())||(!matches!(f.kind,Kind::Integer|Kind::Number)&&(f.min.is_some()||f.max.is_some())){return Err(Error::InvalidBatch);}
    }Ok(())
}
pub fn validate(fields:&[Field],attrs:&Value)->Result<(),Error>{
    validate_schema(fields)?;let object=attrs.as_object().ok_or(Error::InvalidBatch)?;
    if object.keys().any(|k|!fields.iter().any(|f|f.name==*k)){return Err(Error::InvalidBatch);}
    for f in fields{
        let v=object.get(&f.name).unwrap_or(&Value::Null);
        if v.is_null(){if f.required{return Err(Error::InvalidBatch);}continue;}
        let ok=match f.kind{
            Kind::Text=>v.as_str().is_some_and(|s|f.max_length.is_none_or(|n|s.chars().count()<=n)),
            Kind::Boolean=>v.is_boolean(),
            Kind::Integer|Kind::Number=> (matches!(f.kind,Kind::Number)||v.is_i64()||v.is_u64())&&v.as_f64().is_some_and(|n|n.is_finite()&&f.min.is_none_or(|m|n>=m)&&f.max.is_none_or(|m|n<=m))
        };
        if !ok{return Err(Error::Spatial(format!("ATTRIBUTE_CONSTRAINT: {}",f.name)));}
    }Ok(())
}
