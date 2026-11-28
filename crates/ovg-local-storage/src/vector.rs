//! Strict, bounded standalone GeoJSON and CSV candidate adapters. No silent Z/M or field loss.
use super::{LocalStore,Mutation,Error};
use ovg_spatial::{parse_geojson,parse_xy_wkb,encode_wkb,to_geojson,ValidationLimits,PortableTransformKernel,TransformBackend,Coord2D};
use serde_json::{Value,json};
use uuid::Uuid;
const MAX_INPUT:usize=16*1024*1024;
const MAX_ROWS:usize=4096;

fn strict_coords(v:&Value,depth:usize)->Result<(),Error>{
    if depth>6{return Err(Error::Invalid);}
    let a=v.as_array().ok_or(Error::Invalid)?;
    if a.is_empty(){return Err(Error::Invalid);}
    if a[0].is_number(){
        if a.len()!=2||a.iter().any(|x|!x.as_f64().is_some_and(f64::is_finite)){return Err(Error::Invalid);}
    }else{for x in a{strict_coords(x,depth+1)?;}}
    Ok(())
}
fn feature(v:&Value)->Result<Mutation,Error>{
    if v["type"]!="Feature"||v.as_object().ok_or(Error::Invalid)?.keys().any(|k|!matches!(k.as_str(),"type"|"geometry"|"properties"|"id"|"bbox")){return Err(Error::Invalid);}
    let geom=&v["geometry"];
    // No mixed collections, foreign geometry members or Z/M silently flattened.
    let kind=geom["type"].as_str().ok_or(Error::Invalid)?;
    if !matches!(kind,"Point"|"MultiPoint"|"LineString"|"MultiLineString"|"Polygon"|"MultiPolygon")||geom.as_object().ok_or(Error::Invalid)?.keys().any(|k|!matches!(k.as_str(),"type"|"coordinates"|"bbox")){return Err(Error::Invalid);}
    strict_coords(&geom["coordinates"],0)?;
    let geometry=parse_geojson(geom,&ValidationLimits::default()).map_err(|_|Error::Invalid)?;
    let wkb=encode_wkb(&geometry,None).map_err(|_|Error::Invalid)?;
    let attrs=v["properties"].as_object().ok_or(Error::Invalid)?.clone();
    // Preserve canonical UUIDs, reject other explicit IDs rather than silently discard.
    let feature_id=if let Some(value)=v.get("id"){
        let s=value.as_str().ok_or(Error::Invalid)?;
        Uuid::parse_str(s).ok().filter(|u|!u.is_nil()&&u.to_string()==s).ok_or(Error::Invalid)?
    }else{Uuid::new_v4()};
    Ok(Mutation{operation_id:Uuid::new_v4(),feature_id,expected_revision:0,wkb:Some(wkb),attributes:Value::Object(attrs)})
}
impl LocalStore{
    pub fn import_geojson(&self,bytes:&[u8])->Result<u32,Error>{
        if bytes.len()>MAX_INPUT{return Err(Error::Limit);}
        let v:Value=serde_json::from_slice(bytes).map_err(|_|Error::Invalid)?;
        if v["type"]!="FeatureCollection"||v.as_object().ok_or(Error::Invalid)?.keys().any(|k|!matches!(k.as_str(),"type"|"features"|"bbox")){return Err(Error::Invalid);}
        let rows=v["features"].as_array().ok_or(Error::Invalid)?;
        if rows.len()>MAX_ROWS||rows.is_empty(){return Err(Error::Limit);}
        let mutations=rows.iter().map(feature).collect::<Result<Vec<_>,_>>()?;
        self.apply_batch(&mutations)?;Ok(mutations.len() as u32)
    }
    /// RFC 7946 XY, bounded candidate export. Explicit refusal if dataset exceeds one page.
    pub fn export_geojson(&self)->Result<String,Error>{
        let mut db=self.lock()?;
        let tx=db.transaction()?;
        let rows=Self::query_in(&tx,[-180.,-90.,180.,90.],0,MAX_ROWS as u32)?;
        let last=rows.last().map_or(0,|r|r.0);
        if !Self::query_in(&tx,[-180.,-90.,180.,90.],last,1)?.is_empty(){return Err(Error::Limit);}
        let mut features=Vec::with_capacity(rows.len());let mut size=0usize;
        for (_,f) in rows{
            let g=parse_xy_wkb(&f.wkb,&ValidationLimits::default()).map_err(|_|Error::Invalid)?;
            let v=json!({"type":"Feature","id":f.feature_id.to_string(),"geometry":to_geojson(&g.geometry),"properties":f.attributes});
            size=size.checked_add(v.to_string().len()).ok_or(Error::Limit)?;
            if size>MAX_INPUT{return Err(Error::Limit);}features.push(v);
        }
        let out=json!({"type":"FeatureCollection","features":features}).to_string();
        if out.len()>MAX_INPUT{return Err(Error::Limit);}
        tx.commit()?;Ok(out)
    }
    /// UTF-8 CSV, explicit delimiter, coordinate column names and source CRS.
    /// All original fields retained as strings; formula-like cells are never executed.
    pub fn import_csv(&self,bytes:&[u8],x:&str,y:&str,epsg:u32,delimiter:u8)->Result<u32,Error>{
        if bytes.len()>MAX_INPUT||!matches!(delimiter,b','|b';'|b'\t')||x==y{return Err(Error::Invalid);}
        let mut r=csv::ReaderBuilder::new().delimiter(delimiter).from_reader(bytes);
        let headers=r.headers().map_err(|_|Error::Invalid)?.clone();
        let mut names=std::collections::HashSet::new();
        if headers.len()>256||headers.iter().any(|h|h.is_empty()||!names.insert(h)){return Err(Error::Invalid);}
        let xi=headers.iter().position(|h|h==x).ok_or(Error::Invalid)?;
        let yi=headers.iter().position(|h|h==y).ok_or(Error::Invalid)?;
        let mut mutations=Vec::new();let kernel=PortableTransformKernel::new();
        for row in r.records(){
            if mutations.len()>=MAX_ROWS{return Err(Error::Limit);}
            let row=row.map_err(|_|Error::Invalid)?;
            let a=row.get(xi).ok_or(Error::Invalid)?.parse::<f64>().map_err(|_|Error::Invalid)?;
            let b=row.get(yi).ok_or(Error::Invalid)?.parse::<f64>().map_err(|_|Error::Invalid)?;
            let p=kernel.transform_point(epsg,4326,Coord2D::new(a,b)).map_err(|_|Error::Invalid)?;
            let props:serde_json::Map<String,Value>=headers.iter().zip(row.iter()).map(|(k,v)|(k.to_owned(),json!(v))).collect();
            mutations.push(feature(&json!({"type":"Feature","geometry":{"type":"Point","coordinates":[p.x,p.y]},"properties":props}))?);
        }
        self.apply_batch(&mutations)?;Ok(mutations.len() as u32)
    }
}

impl LocalStore{
    /// Small, viewport-scoped source only. Explicit truncation signal; not a large-layer LOD implementation.
    pub fn viewport_geojson(&self,bbox:[f64;4])->Result<String,Error>{
        let mut db=self.lock()?;let tx=db.transaction()?;
        let rows=Self::query_in(&tx,bbox,0,513)?;
        let mut features=Vec::new();let mut size=0usize;let mut last=0i64;
        for (fid,f) in rows{
            if features.len()>=512{break;}
            let g=parse_xy_wkb(&f.wkb,&ValidationLimits::default()).map_err(|_|Error::Invalid)?;
            let v=json!({"type":"Feature","id":f.feature_id.to_string(),"properties":{},"geometry":to_geojson(&g.geometry)});
            let n=v.to_string().len();
            if size+n>1024*1024{break;}
            size+=n;last=fid;features.push(v);
        }
        let truncated=!Self::query_in(&tx,bbox,last,1)?.is_empty();
        tx.commit()?;
        Ok(json!({"data":{"type":"FeatureCollection","features":features},"truncated":truncated}).to_string())
    }
}
