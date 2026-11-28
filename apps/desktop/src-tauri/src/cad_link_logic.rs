//! Durable, explicit WGS84 POINT/LINE link. CAD assets are immutable and every
//! linked publication is one SQLite transaction. No projection by assumption.
use super::*;
use ovg_project_store::{Store,Batch,Edit,Image,Row,cad_link::{Binding,CadLink,CadPublication}};
use std::collections::{HashMap,HashSet};

fn linked_rows(store:&Store,link:&CadLink,cad:&crate::resource_files::Cad,meta:&ovg_project_store::asset_store::AssetMeta,rev:u64)->R<(Vec<Row>,HashMap<String,Uuid>)>{
 let page=store.query(link.layer,[-180.,-90.,180.,90.],0,65,rev).map_err(|_|"STORE_QUERY_FAILED")?;
 if page.next.is_some()||page.rows.len()>64||cad.features().len()>64{return Err("SYNC_BATCH_LIMIT")}
 let bindings:HashMap<_,_>=link.bindings.iter().map(|b|(b.handle.clone(),b.feature)).collect();
 let base:HashMap<_,_>=cad.features().iter().map(|f|(f["properties"]["handle"].as_str().unwrap_or(""),f)).collect();
 for h in base.keys(){if !bindings.contains_key(*h){return Err("CAD_BINDING_MISSING")}}
 for row in &page.rows{
  if let Some(binding)=link.bindings.iter().find(|b|b.feature==row.id){
   let f=base.get(binding.handle.as_str()).ok_or("CAD_TOMBSTONE_RESTORED")?;
   let p=&row.image.attributes;
   if p["cad_root"]!=link.root.to_string()||p["cad_asset"]!=link.asset.to_string()||p["cad_sha256"]!=meta.sha256||p["cad_handle"]!=binding.handle||p["cad_entity"]!=f["properties"]["entity"]||p["cad_layer"]!=f["properties"]["layer"]{return Err("CAD_PROVENANCE_MISMATCH")}
  }else if row.image.attributes.as_object().map_or(true,|o|o.keys().any(|k|k.starts_with("cad_"))){return Err("UNBOUND_CAD_PROVENANCE")}
 }
 for(h,id)in &bindings{
  if base.contains_key(h.as_str())&&!page.rows.iter().any(|r|r.id==*id){
   let (r,img)=store.feature_state(link.layer,*id,rev).map_err(|_|"STORE_QUERY_FAILED")?;
   if r==0||img.is_some(){return Err("CAD_BINDING_MISSING")}
  }
 }
 Ok((page.rows,bindings))
}
fn attrs(mut a:Value,root:Uuid,asset:Uuid,sha:&str,f:&Value)->Value{
 a["cad_root"]=serde_json::json!(root);a["cad_asset"]=serde_json::json!(asset);a["cad_sha256"]=serde_json::json!(sha);
 a["cad_handle"]=f["properties"]["handle"].clone();a["cad_layer"]=f["properties"]["layer"].clone();a["cad_entity"]=f["properties"]["entity"].clone();a
}
fn geometry(row:&Row)->R<Value>{
 let g=ovg_spatial::parse_xy_wkb(&row.image.wkb,&ovg_spatial::ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?;
 let g=ovg_spatial::to_geojson(&g.geometry);replacement(&g)?;Ok(g)
}
fn get_handle(f:&Value)->R<&str>{f["properties"]["handle"].as_str().ok_or("INVALID_DXF_IDENTITY")}
fn has_custom(a:&Value)->bool{a.as_object().map_or(true,|o|o.keys().any(|k|!matches!(k.as_str(),"cad_root"|"cad_asset"|"cad_sha256"|"cad_handle"|"cad_layer"|"cad_entity")))}
pub(super) fn register(store:&Store,layer:Uuid,asset:Uuid,rev:u64,automatic:bool)->R<u64>{
 let(_d,_p,meta,cad)=load_dxf_profile(store,asset,rev,true)?;
 if cad.features().is_empty(){return Err("LINK_REQUIRES_CONVERGED_BASE")}
 let rows=projection_rows(store,layer,asset,rev,&meta,&cad)?;
 let mut bindings=Vec::new();
 for row in rows{
  let h=row.image.attributes["cad_handle"].as_str().ok_or("CAD_PROVENANCE_REQUIRED")?;
  let f=cad.features().iter().find(|f|f["properties"]["handle"]==h).ok_or("CAD_BINDING_MISSING")?;
  if row.image.wkb!=cad_wkb(&f["geometry"])?{return Err("LINK_REQUIRES_CONVERGED_BASE")}
  bindings.push(Binding{handle:h.into(),feature:row.id});
 }
 store.register_cad_link(layer,asset,rev,&bindings,automatic).map_err(|_|"CAD_LINK_REGISTRATION_FAILED")
}
fn forward(store:&Store,link:&CadLink,child:Uuid,rev:u64,operation:Uuid)->R<Value>{
 let(_d,_p,meta,base)=load_dxf_profile(store,link.asset,rev,true)?;
 let(_e,_q,next,updated)=load_dxf_profile(store,child,rev,true)?;
 let(rows,bindings)=linked_rows(store,link,&base,&meta,rev)?;
 if updated.features().len()>64{return Err("SYNC_BATCH_LIMIT")}
 let old:HashMap<_,_>=base.features().iter().map(|f|Ok((get_handle(f)?,f))).collect::<R<_>>()?;
 let new:HashMap<_,_>=updated.features().iter().map(|f|Ok((get_handle(f)?,f))).collect::<R<_>>()?;
 let mut edits=Vec::new();let mut additions=Vec::new();
 // Refuse independent GIS additions when a CAD change is pending.
 for row in &rows{if !bindings.values().any(|id|*id==row.id){return Err("CONCURRENT_GIS_ADDITION")}}
 for(h,before)in &old{
  let id=bindings[*h];let row=rows.iter().find(|r|r.id==id);
  match (row,new.get(h)){
   (Some(row),Some(after))=>{
    if before["properties"]["entity"]!=after["properties"]["entity"]{return Err("SYNC_ENTITY_TYPE_CHANGED")}
    let target=cad_wkb(&after["geometry"])?;
    if row.image.wkb!=cad_wkb(&before["geometry"])?&&row.image.wkb!=target{return Err("GIS_GEOMETRY_CONFLICT")}
    edits.push(Edit{layer:link.layer,feature:id,expected:row.revision,image:Some(Image{wkb:target,attributes:attrs(row.image.attributes.clone(),link.root,child,&next.sha256,after)})});
   },
   (Some(row),None)=>{
    if row.image.wkb!=cad_wkb(&before["geometry"])?||has_custom(&row.image.attributes){return Err("GIS_DELETE_CONFLICT")}
    edits.push(Edit{layer:link.layer,feature:id,expected:row.revision,image:None});
   },
   (None,Some(_))=>return Err("CONCURRENT_GIS_DELETION"),
   (None,None)=>{}
  }
 }
 for(h,f)in &new{
  if old.contains_key(h){continue}
  if bindings.contains_key(*h){return Err("CAD_HANDLE_REUSE")}
  let id=cad_feature_id(link.root,h);
  if store.feature_state(link.layer,id,rev).map_err(|_|"STORE_QUERY_FAILED")?.0!=0{return Err("CAD_HANDLE_REUSE")}
  additions.push(Binding{handle:h.to_string(),feature:id});
  edits.push(Edit{layer:link.layer,feature:id,expected:0,image:Some(Image{wkb:cad_wkb(&f["geometry"])?,attributes:attrs(serde_json::json!({}),link.root,child,&next.sha256,f)})});
 }
 if edits.len()>64{return Err("SYNC_BATCH_LIMIT")}
 let n=store.commit_cad_link(link.layer,link.asset,child,None,&Batch{operation,expected_project:rev,edits,links:vec![]},&additions,&AtomicBool::new(false)).map_err(|_|"CAD_LINK_COMMIT_FAILED")?;
 Ok(serde_json::json!({"direction":"cad_to_gis","asset":child,"revision":n.to_string()}))
}

// Bounded entity-set rewrite preserves every untouched group record byte-for-byte.
// Creation uses the existing model-space owner when present, and layer 0 only.
// Refuse ambiguous owners, object references to removed handles and unsupported
// modern drawings without a model-space owner for newly created entities.
fn rewrite_set(src:&Path,out:&Path,delete:&HashSet<String>,create:&[(Uuid,Value)],reserved:&HashSet<String>)->R<Vec<Binding>>{
 let raw=fs::read_to_string(src).map_err(|_|"FILE_UNAVAILABLE")?;
 if raw.len()>4*1024*1024{return Err("RESOURCE_LIMIT")}
 let lines:Vec<_>=raw.split_inclusive('\n').collect();
 if lines.len()%2!=0{return Err("INVALID_DXF")}
 let codes:Vec<u16>=lines.chunks_exact(2).map(|p|p[0].trim().parse().map_err(|_|"INVALID_DXF")).collect::<R<_>>()?;
 let val=|i:usize|lines[i*2+1].trim();
 let mut used:HashSet<String>=reserved.clone();let mut max=0u64;let mut seed=None;let mut modern=false;
 for i in 0..codes.len(){
  if codes[i]==9&&val(i)=="$ACADVER"&&codes.get(i+1)==Some(&1){modern=val(i+1)>"AC1009"}
  if codes[i]==9&&val(i)=="$HANDSEED"&&codes.get(i+1)==Some(&5){if seed.replace(i+1).is_some(){return Err("INVALID_HANDSEED")}}
  if matches!(codes[i],5|105){let h=val(i).to_ascii_uppercase();let n=u64::from_str_radix(&h,16).map_err(|_|"INVALID_DXF_HANDLE")?;max=max.max(n);used.insert(h);}
 }
 for h in reserved{max=max.max(u64::from_str_radix(h,16).map_err(|_|"INVALID_DXF_HANDLE")?)}
 let mut skip=HashSet::new();let mut owners=HashSet::new();let mut removed=HashSet::new();let mut missing_owner=false;let mut ent=false;let mut end=None;let mut i=0;
 while i<codes.len(){
  if codes[i]==0&&val(i)=="SECTION"&&codes.get(i+1)==Some(&2)&&val(i+1)=="ENTITIES"{if ent||end.is_some(){return Err("INVALID_DXF")}ent=true;i+=2;continue}
  if ent&&codes[i]==0&&val(i)=="ENDSEC"{ent=false;end=Some(i);i+=1;continue}
  if !ent{i+=1;continue}
  if codes[i]!=0{return Err("INVALID_DXF")}
  let start=i;i+=1;while i<codes.len()&&codes[i]!=0{i+=1}
  if (start+1..i).any(|j|codes[j]==67&&val(j)!="0"){return Err("PAPER_SPACE_UNSUPPORTED")}
  let hs:Vec<_>=(start+1..i).filter(|j|codes[*j]==5).collect();if hs.len()!=1{return Err("INVALID_DXF_IDENTITY")}
  let h=val(hs[0]).to_ascii_uppercase();
  let os:Vec<_>=(start+1..i).filter(|j|codes[*j]==330).collect();
  if os.len()>1{return Err("CAD_OWNER_AMBIGUOUS")}
  if os.is_empty(){missing_owner=true}
  if let Some(j)=os.first(){let owner=val(*j);if owner.is_empty()||owner.len()>16||!owner.bytes().all(|c|c.is_ascii_hexdigit()){return Err("CAD_OWNER_AMBIGUOUS")}owners.insert(owner.to_ascii_uppercase());}
  if delete.contains(&h){
   if (start+1..i).any(|j|codes[j]==102||codes[j]>=1000){return Err("CAD_DELETE_EXTENSION_CONFLICT")}
   removed.insert(h);skip.extend(start..i);
  }
 }
 if removed!=*delete||ent||end.is_none(){return Err("CAD_ENTITY_SET_MISMATCH")}
 for i in 0..codes.len(){
  if !skip.contains(&i)&&matches!(codes[i],320..=369|390..=399|480|481|1005)&&delete.contains(&val(i).to_ascii_uppercase()){return Err("CAD_DELETE_REFERENCE_CONFLICT")}
 }
 if !create.is_empty()&&(owners.len()>1||(modern&&(owners.is_empty()||missing_owner))){return Err("CAD_OWNER_AMBIGUOUS")}
 let eol=if lines.iter().any(|l|l.ends_with("\r\n")){"\r\n"}else{"\n"};
 let mut inserted=String::new();let mut bindings=Vec::new();
 for(id,g)in create{
  let mut h;loop{max=max.checked_add(1).filter(|n|*n<u64::MAX).ok_or("CAD_HANDLE_EXHAUSTED")?;h=format!("{max:X}");if used.insert(h.clone()){break}}
  let kind=match g["type"].as_str(){Some("Point")=>"POINT",Some("LineString")=>"LINE",_=>return Err("UNSUPPORTED_DXF_ENTITY")};
  let mut pair=|c:u16,v:&str|{inserted.push_str(&format!("{c}{eol}{v}{eol}"))};
  pair(0,kind);pair(5,&h);
  if let Some(owner)=owners.iter().next(){pair(330,owner)}
  if modern{pair(100,"AcDbEntity")}
  pair(8,"0");if modern{pair(100,if kind=="POINT"{"AcDbPoint"}else{"AcDbLine"})}
  for(c,v)in replacement(g)?{pair(c,&v.to_string())}
  bindings.push(Binding{handle:h,feature:*id});
 }
 let mut result=String::with_capacity(raw.len()+inserted.len());
 for i in 0..codes.len(){
  if Some(i)==end{result.push_str(&inserted)}
  if skip.contains(&i){continue}
  result.push_str(lines[2*i]);
  if !create.is_empty()&&Some(i)==seed{result.push_str(&format!("{:X}{eol}",max.checked_add(1).ok_or("CAD_HANDLE_EXHAUSTED")?))}else{result.push_str(lines[2*i+1])}
 }
 if result.len()>4*1024*1024{return Err("RESOURCE_LIMIT")}
 fs::write(out,result).map_err(|_|"FILE_UNAVAILABLE")?;Ok(bindings)
}
fn reverse(store:&Store,link:&CadLink,rev:u64,operation:Uuid)->R<Value>{
 let(dir,mut path,meta,base)=load_dxf_profile(store,link.asset,rev,true)?;
 let(rows,bindings)=linked_rows(store,link,&base,&meta,rev)?;
 let mut delete=HashSet::new();let mut create=Vec::new();let mut changed=0;
 for f in base.features(){
  let h=get_handle(f)?;let id=bindings[h];
  if let Some(row)=rows.iter().find(|r|r.id==id){
   if row.image.wkb==cad_wkb(&f["geometry"])?{continue}
   let g=geometry(row)?;let current=crate::resource_files::Cad::open(&path)?;
   let out=dir.path().join(format!("edit-{changed}.dxf"));patch_dxf(&path,&out,&current,h,&g)?;path=out;changed+=1;
  }else{delete.insert(h.to_string());}
 }
 for row in &rows{if !bindings.values().any(|id|*id==row.id){create.push((row.id,geometry(row)?));}}
 if changed==0&&delete.is_empty()&&create.is_empty(){return Ok(serde_json::json!({"direction":"unchanged","asset":link.asset,"revision":rev.to_string()}))}
 let mut additions=Vec::new();
 if !delete.is_empty()||!create.is_empty(){
  let out=dir.path().join("entities.dxf");
  additions=rewrite_set(&path,&out,&delete,&create,&bindings.keys().cloned().collect())?;path=out;
 }
 let after=crate::resource_files::Cad::open_linked(&path)?;
 if after.features().len()!=rows.len(){return Err("CAD_FIDELITY_FAILED")}
 let id=Uuid::new_v4();let sha=digest(&path,MAX)?.1;
 let mut edits=Vec::new();
 for row in rows{
  let h=link.bindings.iter().chain(&additions).find(|b|b.feature==row.id).ok_or("CAD_BINDING_MISSING")?.handle.as_str();
  let f=after.features().iter().find(|f|f["properties"]["handle"]==h).ok_or("CAD_FIDELITY_FAILED")?;
  if cad_wkb(&f["geometry"])?!=row.image.wkb{return Err("CAD_FIDELITY_FAILED")}
  if let Some(before)=base.features().iter().find(|f|f["properties"]["handle"]==h){if before["properties"]!=f["properties"]{return Err("CAD_FIDELITY_FAILED")}}
  edits.push(Edit{layer:link.layer,feature:row.id,expected:row.revision,image:Some(Image{wkb:row.image.wkb,attributes:attrs(row.image.attributes,link.root,id,&sha,f)})});
 }
 let n=store.commit_cad_link(link.layer,link.asset,id,Some(CadPublication{id,path:&path,sha:&sha,parent_sha:&meta.sha256}),&Batch{operation,expected_project:rev,edits,links:vec![]},&additions,&AtomicBool::new(false)).map_err(|_|"CAD_LINK_COMMIT_FAILED")?;
 Ok(serde_json::json!({"direction":"gis_to_cad","asset":id,"revision":n.to_string()}))
}
pub(super) fn reconcile(store:&Store,layer:Uuid,rev:u64,operation:Uuid,automatic:bool)->R<Value>{
 let link=store.cad_link(layer,rev).map_err(|_|"STORE_QUERY_FAILED")?.ok_or("CAD_LINK_REQUIRED")?;
 if automatic&&!link.automatic{return Err("CAD_AUTOMATIC_DISABLED")}
 let versions=store.cad_versions().map_err(|_|"STORE_QUERY_FAILED")?;
 let children:Vec<_>=versions.iter().filter(|v|v.parent==link.asset).collect();
 match children.as_slice(){[]=>reverse(store,&link,rev,operation),[child]=>forward(store,&link,child.asset,rev,operation),_=>Err("CAD_BRANCH_CONFLICT")}
}

#[cfg(test)] mod tests{
 use super::*;
 fn drawing(items:&[(&str,f64)])->String{
  format!("0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",items.iter().map(|(h,x)|format!("0\nPOINT\n5\n{h}\n8\n0\n10\n{x}\n20\n11\n")).collect::<String>())
 }
 fn fixture()->(tempfile::TempDir,Store,Uuid,Uuid){
  let d=tempfile::tempdir().unwrap();let p=d.path().join("base.dxf");fs::write(&p,drawing(&[("AB",106.),("AC",107.)])).unwrap();
  let s=Store::create(&d.path().join("p.sqlite")).unwrap();let a=Uuid::new_v4();let l=Uuid::new_v4();let hash=digest(&p,MAX).unwrap().1;
  s.attach_cad_version(a,"cad-native-dxf",&p,0,&hash,None,&AtomicBool::new(false)).unwrap();
  project_dxf(&s,a,1,Uuid::new_v4(),l,"linked").unwrap();assert_eq!(register(&s,l,a,2,true).unwrap(),3);(d,s,a,l)
 }
 #[test]fn roundtrip_add_delete_preserves_uuid_and_durable_link(){
  let(d,s,a,l)=fixture();let f=cad_feature_id(a,"AB");let added=Uuid::new_v4();
  let w=cad_wkb(&serde_json::json!({"type":"Point","coordinates":[108.,11.]})).unwrap();
  s.apply(&Batch{operation:Uuid::new_v4(),expected_project:3,edits:vec![Edit{layer:l,feature:f,expected:1,image:None},Edit{layer:l,feature:added,expected:0,image:Some(Image{wkb:w.clone(),attributes:serde_json::json!({"note":"keep"})})}],links:vec![]}).unwrap();
  let r=reconcile(&s,l,4,Uuid::new_v4(),true).unwrap();assert_eq!(r["direction"],"gis_to_cad");assert_eq!(s.revision().unwrap(),5);
  let link=s.cad_link(l,5).unwrap().unwrap();let h=link.bindings.iter().find(|b|b.feature==added).unwrap().handle.clone();
  assert_eq!(s.feature(l,added,5).unwrap().1.attributes["cad_handle"],h);assert_eq!(s.feature(l,added,5).unwrap().1.attributes["note"],"keep");
  let p=d.path().join("export.dxf");let meta=s.materialize_asset(link.asset,&p,&AtomicBool::new(false)).unwrap();
  let after=crate::resource_files::Cad::open(&p).unwrap();assert_eq!(after.count(),2);assert!(after.features().iter().all(|v|v["properties"]["handle"]!="AB"));
  assert_eq!(reconcile(&s,l,5,Uuid::new_v4(),true).unwrap()["direction"],"unchanged");assert_eq!(s.revision().unwrap(),5);
  // A CAD-side geometry update for the newly assigned handle must retain the original GIS UUID.
  let next=d.path().join("next.dxf");patch_dxf(&p,&next,&after,&h,&serde_json::json!({"type":"Point","coordinates":[109.,11.]})).unwrap();
  let child=Uuid::new_v4();let sha=digest(&next,MAX).unwrap().1;
  s.attach_cad_version(child,"cad-native-dxf",&next,5,&sha,Some((link.asset,&meta.sha256)),&AtomicBool::new(false)).unwrap();
  assert_eq!(reconcile(&s,l,6,Uuid::new_v4(),true).unwrap()["direction"],"cad_to_gis");
  assert_eq!(s.feature(l,added,7).unwrap().1.attributes["note"],"keep");
  assert_eq!(s.feature(l,added,7).unwrap().1.wkb,cad_wkb(&serde_json::json!({"type":"Point","coordinates":[109.,11.]})).unwrap());
  assert!(s.integrity().unwrap());assert_eq!(s.compensate_latest(Uuid::new_v4(),7),Err(ovg_project_store::Error::Unsupported));s.checkpoint().unwrap();drop(s);
  let s=Store::reopen(&d.path().join("p.sqlite")).unwrap();assert_eq!(s.cad_link(l,7).unwrap().unwrap().asset,child);assert!(s.integrity().unwrap());
 }
 #[test]fn simultaneous_branches_fail_without_writes(){
  let(d,s,a,l)=fixture();let sha=s.assets().unwrap().into_iter().find(|v|v.id==a).unwrap().sha256;
  let p=d.path().join("fork.dxf");fs::write(&p,drawing(&[("AB",105.),("AC",107.)])).unwrap();
  s.attach_cad_version(Uuid::new_v4(),"cad-native-dxf",&p,3,&digest(&p,MAX).unwrap().1,Some((a,&sha)),&AtomicBool::new(false)).unwrap();
  let f=cad_feature_id(a,"AB");let(fr,mut img)=s.feature(l,f,4).unwrap();img.wkb=cad_wkb(&serde_json::json!({"type":"Point","coordinates":[109.,11.]})).unwrap();
  s.apply(&Batch{operation:Uuid::new_v4(),expected_project:4,edits:vec![Edit{layer:l,feature:f,expected:fr,image:Some(img)}],links:vec![]}).unwrap();
  assert_eq!(reconcile(&s,l,5,Uuid::new_v4(),true),Err("GIS_GEOMETRY_CONFLICT"));assert_eq!(s.revision().unwrap(),5);assert_eq!(s.cad_link(l,5).unwrap().unwrap().asset,a);
  fs::write(&p,drawing(&[("AB",104.),("AC",107.)])).unwrap();
  s.attach_cad_version(Uuid::new_v4(),"cad-native-dxf",&p,5,&digest(&p,MAX).unwrap().1,Some((a,&sha)),&AtomicBool::new(false)).unwrap();
  assert_eq!(reconcile(&s,l,6,Uuid::new_v4(),true),Err("CAD_BRANCH_CONFLICT"));assert_eq!(s.revision().unwrap(),6);
 }
 #[test]fn delete_all_and_recreate_never_reuses_handle(){
  let(_d,s,a,l)=fixture();
  s.apply(&Batch{operation:Uuid::new_v4(),expected_project:3,edits:["AB","AC"].iter().map(|h|Edit{layer:l,feature:cad_feature_id(a,h),expected:1,image:None}).collect(),links:vec![]}).unwrap();
  reconcile(&s,l,4,Uuid::new_v4(),true).unwrap();let link=s.cad_link(l,5).unwrap().unwrap();
  assert_eq!(link.bindings.len(),2);assert_eq!(reconcile(&s,l,5,Uuid::new_v4(),true).unwrap()["direction"],"unchanged");
  let id=Uuid::new_v4();let img=Image{wkb:cad_wkb(&serde_json::json!({"type":"Point","coordinates":[110.,11.]})).unwrap(),attributes:serde_json::json!({})};
  s.apply(&Batch{operation:Uuid::new_v4(),expected_project:5,edits:vec![Edit{layer:l,feature:id,expected:0,image:Some(img)}],links:vec![]}).unwrap();
  reconcile(&s,l,6,Uuid::new_v4(),true).unwrap();
  let lnk=s.cad_link(l,7).unwrap().unwrap();assert_eq!(lnk.bindings.iter().find(|b|b.feature==id).unwrap().handle,"AD");assert!(s.integrity().unwrap());
 }
 #[test]fn referenced_handle_and_stale_revision_fail_closed(){
  let d=tempfile::tempdir().unwrap();let p=d.path().join("src.dxf");let out=d.path().join("out.dxf");
  let src=drawing(&[("AB",106.),("AC",107.)]).replace("0\nEOF\n","0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nFF\n350\nAB\n0\nENDSEC\n0\nEOF\n");
  fs::write(&p,src).unwrap();
  assert_eq!(rewrite_set(&p,&out,&HashSet::from(["AB".into()]),&[],&HashSet::new()),Err("CAD_DELETE_REFERENCE_CONFLICT"));
  let(_d,s,_a,l)=fixture();assert_eq!(reconcile(&s,l,2,Uuid::new_v4(),true),Err("STORE_QUERY_FAILED"));
 }
}
