//! Frozen tool contract from the canonical MCP adapter. All validation is
//! native; no Python subprocess is involved in request processing.
use serde_json::{json,Value,Map};
use regex::Regex;
use uuid::Uuid;
use crate::Error;
use std::collections::{HashMap,HashSet};

pub struct Registry { tools: Vec<Value> }
impl Registry {
    pub fn new()->Self {
        let mut tools:Vec<Value>=serde_json::from_str(include_str!("../contracts/workspace-tools.json")).expect("checked-in contracts");
        tools.extend([
            json!({"name":"gis_live_view","description":"Read the connected live GIS viewport after user approval.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false}}),
            json!({"name":"gis_live_selection","description":"Read current selected WGS84 geometry after approval.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false}}),
            json!({"name":"gis_live_navigate","description":"Move the connected live camera after approval.","inputSchema":{"type":"object","properties":{"longitude":{"type":"number","minimum":-180,"maximum":180},"latitude":{"type":"number","minimum":-85,"maximum":85},"zoom":{"type":"number","minimum":0,"maximum":22}},"required":["longitude","latitude","zoom"],"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false}}),
            json!({"name":"gis_operation_status","description":"Read the status of a prior operation ticket in the same workspace.","inputSchema":{"type":"object","properties":{"operation_ticket":{"type":"string","format":"uuid"}},"required":["operation_ticket"],"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false}}),
            json!({"name":"gis_workspaces","description":"List your connected GIS workspaces. When more than one exists, supply the exact session_id on subsequent tool calls.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false}})
        ]);
        for t in &mut tools {
            if t["name"]!="gis_workspaces" {
                t["inputSchema"]["properties"]["session_id"]=json!({"type":"string","format":"uuid","description":"Exact connected workspace ID; required if multiple are connected."});
            }
            t["inputSchema"]["additionalProperties"]=json!(false);
        }
        Self{tools}
    }
    pub fn list(&self)->&[Value]{&self.tools}
    pub fn call(&self,name:&str,args:&Value)->Result<(Option<Uuid>,Value),Error>{
        if !args.is_object()||!super::bounded(args){return Err(Error::Invalid)}
        let tool=self.tools.iter().find(|t|t["name"]==name).ok_or(Error::Invalid)?;
        let normalized=validate(args,&tool["inputSchema"],&tool["inputSchema"],0)?;
        let target=normalized.get("session_id").and_then(Value::as_str).map(Uuid::parse_str).transpose().map_err(|_|Error::Invalid)?;
        let mut params=if let Some(req)=normalized.get("request"){req.clone()}else{normalized};
        if let Some(o)=params.as_object_mut(){o.remove("session_id");}
        cross_check(name,&params)?;
        Ok((target,params))
    }
}
impl Default for Registry {fn default()->Self{Self::new()}}
fn validate(v:&Value,s:&Value,root:&Value,depth:usize)->Result<Value,Error>{
    if depth>64{return Err(Error::Invalid)}
    if let Some(path)=s.get("$ref").and_then(Value::as_str){
        return validate(v,root.pointer(path.strip_prefix('#').ok_or(Error::Invalid)?).ok_or(Error::Invalid)?,root,depth+1)
    }
    if let Some(choices)=s.get("anyOf").and_then(Value::as_array){
        return choices.iter().find_map(|c|validate(v,c,root,depth+1).ok()).ok_or(Error::Invalid)
    }
    if s.get("const").is_some_and(|c|c!=v)||s.get("enum").and_then(Value::as_array).is_some_and(|a|!a.contains(v)){return Err(Error::Invalid)}
    match s.get("type").and_then(Value::as_str){
        Some("object")=>{
            let obj=v.as_object().ok_or(Error::Invalid)?;
            let mut out=Map::new();
            let props=s.get("properties").and_then(Value::as_object);
            if let Some(required)=s.get("required").and_then(Value::as_array){for key in required{if !obj.contains_key(key.as_str().ok_or(Error::Invalid)?){return Err(Error::Invalid)}}}
            for (k,val) in obj{
                let n=if let Some(schema)=props.and_then(|p|p.get(k)){validate(val,schema,root,depth+1)?}else{
                    match s.get("additionalProperties"){
                        Some(Value::Bool(false))=>return Err(Error::Invalid),
                        Some(other) if other.is_object()=>validate(val,other,root,depth+1)?,
                        _=>val.clone()
                    }
                };
                if !n.is_null(){out.insert(k.clone(),n);}
            }
            if let Some(p)=props{for(k,schema)in p{if !out.contains_key(k)&&!obj.contains_key(k){if let Some(default)=schema.get("default").filter(|v|!v.is_null()){out.insert(k.clone(),validate(default,schema,root,depth+1)?);}}}}
            Ok(Value::Object(out))
        }
        Some("array")=>{
            let a=v.as_array().ok_or(Error::Invalid)?;let n=a.len() as u64;
            if s["minItems"].as_u64().is_some_and(|x|n<x)||s["maxItems"].as_u64().is_some_and(|x|n>x){return Err(Error::Invalid)}
            Ok(Value::Array(a.iter().map(|x|validate(x,&s["items"],root,depth+1)).collect::<Result<_,_>>()?))
        }
        Some("string")=>{
            let x=v.as_str().ok_or(Error::Invalid)?;let n=x.chars().count() as u64;
            if s["minLength"].as_u64().is_some_and(|m|n<m)||s["maxLength"].as_u64().is_some_and(|m|n>m){return Err(Error::Invalid)}
            if let Some(p)=s["pattern"].as_str(){if !Regex::new(p).map_err(|_|Error::Invalid)?.is_match(x){return Err(Error::Invalid)}}
            if s["format"]=="uuid"{return Ok(json!(Uuid::parse_str(x).map_err(|_|Error::Invalid)?.to_string()))}
            Ok(v.clone())
        }
        Some("number")|Some("integer")=>{
            if s["type"]=="integer"&&!v.is_i64()&&!v.is_u64(){return Err(Error::Invalid)}
            let n=v.as_f64().filter(|x|x.is_finite()).ok_or(Error::Invalid)?;
            if s["minimum"].as_f64().is_some_and(|m|n<m)||s["maximum"].as_f64().is_some_and(|m|n>m)||s["exclusiveMinimum"].as_f64().is_some_and(|m|n<=m){return Err(Error::Invalid)}Ok(v.clone())
        }
        Some("boolean") if v.is_boolean()=>Ok(v.clone()),
        Some("null") if v.is_null()=>Ok(v.clone()),
        None=>Ok(v.clone()),
        _=>Err(Error::Invalid)
    }
}
fn has(v:&Value,key:&str)->bool{v.get(key).is_some_and(|x|!x.is_null())}
fn text<'a>(v:&'a Value,key:&str)->&'a str{v[key].as_str().unwrap_or("")}
fn n(v:&Value,key:&str)->f64{v[key].as_f64().unwrap_or(0.0)}
fn arr<'a>(v:&'a Value,key:&str)->&'a [Value]{v[key].as_array().map(Vec::as_slice).unwrap_or(&[])}
fn unique(v:&[Value],key:&str)->bool{let mut ids=HashSet::new();v.iter().all(|x|ids.insert(text(x,key)))}
fn geometry(g:&Value)->Result<(),Error>{
    let depth=match text(g,"type"){"Point"=>0,"MultiPoint"|"LineString"=>1,"MultiLineString"|"Polygon"=>2,"MultiPolygon"=>3,_=>return Err(Error::Invalid)};
    let mut count=0;let mut nodes=0;
    fn walk(v:&Value,d:u8,count:&mut u32,nodes:&mut u32)->Result<(),Error>{
        *nodes+=1;let a=v.as_array().ok_or(Error::Invalid)?;
        if *nodes>4096||a.is_empty()||a.len()>2048{return Err(Error::Invalid)}
        if d==0{*count+=1;if *count>2048||a.len()!=2{return Err(Error::Invalid)};bounds(&a[0],-180.,180.)?;bounds(&a[1],-90.,90.)?;}else{for x in a{walk(x,d-1,count,nodes)?;}}Ok(())
    }
    walk(&g["coordinates"],depth,&mut count,&mut nodes)
}
fn bounds(v:&Value,min:f64,max:f64)->Result<(),Error>{if v.as_f64().is_some_and(|x|x.is_finite()&&x>=min&&x<=max){Ok(())}else{Err(Error::Invalid)}}
fn cross_check(name:&str,v:&Value)->Result<(),Error>{
    let invalid=match name{
        "gis_changeset"=>{let ops=arr(v,"operations");if !unique(ops,"feature_id"){return Err(Error::Invalid)}
            for x in ops{let(g,p)=(has(x,"geometry"),has(x,"properties"));if match text(x,"op_type"){"delete"=>g||p,"create"=>!g||!p,"update"=>!g&&!p,_=>true}{return Err(Error::Invalid)}if g{geometry(&x["geometry"])?;}}false},
        "gis_overlay"=>{for g in arr(v,"geometries"){geometry(g)?;}false},
        "gis_geometry_preview"=>arr(&v["primary"],"feature_ids").is_empty(),
        "gis_raster_process"=>{let m=text(v,"method");(has(v,"second_id")&&!["ndvi","ndwi","ndbi","calculator","rgb","change","mosaic"].contains(&m))||(has(v,"third_id")&&m!="rgb")||(m=="mosaic"&&!has(v,"second_id"))||(has(v,"formula")!=(m=="calculator"))||(has(v,"clip_geojson")!=(m=="clip"))||(has(v,"factor")!=(m=="resample"))||(has(v,"resampling")!=(m=="resample"))},
        "gis_raster_report"=>(has(v,"zones")!=(text(v,"method")=="zonal"))||(has(v,"interval")!=(text(v,"method")=="contour")),
        "gis_processing_run"=>has(v,"overlay_layer_id")!=(text(v,"algorithm_id")=="clip"),
        "gis_model_save"=>{if has(v,"id")!=has(v,"revision"){return Err(Error::Invalid)}
            let nodes=arr(&v["graph"],"nodes");if !unique(nodes,"id"){return Err(Error::Invalid)}
            let by:HashMap<_,_>=nodes.iter().map(|x|(text(x,"id"),x)).collect();let vector=|x:&Value|["buffer","clip","dissolve"].contains(&text(x,"algorithm"));let mut done=HashSet::new();
            for node in nodes{for k in ["a","b","c"]{let r=&node[k];if text(r,"type")=="step"{let src=by.get(text(r,"id")).ok_or(Error::Invalid)?;if vector(src)!=vector(node){return Err(Error::Invalid)}}}}
            while done.len()<nodes.len(){let before=done.len();for node in nodes{if ["a","b","c"].iter().all(|k|text(&node[k],"type")!="step"||done.contains(text(&node[k],"id"))){done.insert(text(node,"id"));}}if before==done.len(){return Err(Error::Invalid)}}false},
        "gis_layout_save"=>{if has(v,"id")!=has(v,"revision"){return Err(Error::Invalid)}
            let d=&v["document"];let mut wh=match text(d,"paper"){"A0"=>(841.,1189.),"A1"=>(594.,841.),"A2"=>(420.,594.),"A3"=>(297.,420.),_=>(210.,297.)};if d["landscape"]==true{wh=(wh.1,wh.0)}
            let items=arr(d,"items");let maps:HashSet<_>=items.iter().filter(|x|text(x,"kind")=="map").map(|x|text(x,"id")).collect();
            if !unique(items,"id")||maps.is_empty()||maps.len()>4||(!["A3","A4"].contains(&text(d,"paper"))&&d["dpi"]!=150){return Err(Error::Invalid)}
            for x in items{let kind=text(x,"kind");if n(x,"x")+n(x,"w")>wh.0+0.0001||n(x,"y")+n(x,"h")>wh.1+0.0001||has(x,"camera")!=(kind=="map")||if ["north","scale"].contains(&kind){!maps.contains(text(x,"map_id"))}else{has(x,"map_id")}{return Err(Error::Invalid)}}false},
        _=>false
    };
    if invalid{Err(Error::Invalid)}else{Ok(())}
}
pub fn dispatch(name:&str)->(&str,u8,u64){
    let method=match name{"gis_live_view"=>"map_view","gis_live_selection"=>"map_selection","gis_live_navigate"=>"map_navigate",x=>x};
    let advanced=["gis_raster_layers","gis_raster_process","gis_raster_report","gis_raster_download","gis_processing_run","gis_processing_job","gis_models","gis_model_save","gis_model_run","gis_layouts","gis_layout_save","gis_layout_export","gis_plugins","gis_plugin_activate","gis_plugin_disable","gis_plugin_records","gis_plugin_create","gis_plugin_update"];
    let version=if method.starts_with("map_"){0}else if advanced.contains(&method){2}else{1};
    let ttl=match method{"gis_model_run"=>840_000,"gis_layout_export"=>180_000,"gis_raster_process"|"gis_raster_report"|"gis_raster_download"|"gis_processing_run"=>150_000,_=>40_000};
    (method,version,ttl)
}
pub fn validate_result(method:&str,result:&Value)->Result<(),Error>{
    if !super::bounded(result){return Err(Error::Invalid)}
    match method{
        "map_navigate"=>if result.is_null(){Ok(())}else{Err(Error::Invalid)},
        "map_view"=>{if result["crs"]!="EPSG:4326"||arr(result,"center").len()!=2||arr(result,"bbox").len()!=4{return Err(Error::Invalid)}
            let c=arr(result,"center");let b=arr(result,"bbox");bounds(&c[0],-180.,180.)?;bounds(&c[1],-90.,90.)?;bounds(&b[0],-10000.,10000.)?;bounds(&b[1],-90.,90.)?;bounds(&b[2],-10000.,10000.)?;bounds(&b[3],-90.,90.)?;bounds(&result["zoom"],0.,22.)},
        "map_selection"=>if result["crs"]=="EPSG:4326"{geometry(&result["geometry"])}else{Err(Error::Invalid)},
        _=>if result.is_object(){Ok(())}else{Err(Error::Invalid)}
    }
}
#[cfg(test)]mod tests{use super::*;
#[test]fn registry_and_validation(){let r=Registry::new();assert_eq!(r.list().len(),37);assert_eq!(r.call("gis_layers",&json!({"request":{}})).unwrap().1,json!({"limit":50}));assert!(r.call("gis_layers",&json!({"request":{"limit":101}})).is_err());assert!(r.call("gis_live_navigate",&json!({"longitude":181,"latitude":0,"zoom":2})).is_err());assert!(r.call("gis_overlay",&json!({"request":{"title":"a","geometries":[{"type":"Point","coordinates":[181,0]}]}})).is_err());}
#[test]fn strict_required_and_uuid(){let r=Registry::new();assert!(r.call("gis_processing_job",&json!({"request":{"job_id":"not-uuid"}})).is_err());assert!(r.call("gis_layers",&json!({"request":{"raw_sql":"select *"}})).is_err());assert!(r.call("gis_live_view",&json!({"unexpected":true})).is_err());}
}
