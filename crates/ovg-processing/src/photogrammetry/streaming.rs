//! Bounded viewport planning and revision-scoped range authorization.
//! The desktop host must authenticate the caller and resolve handles to verified assets.
use std::collections::BTreeSet;

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub struct Key{pub z:u8,pub x:u32,pub y:u32}
#[derive(Debug,Clone,Copy)]
pub struct View{pub z:u8,pub min_x:u32,pub min_y:u32,pub max_x:u32,pub max_y:u32}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit,Unauthorized,Stale,Overflow}
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct Plan{pub visible:Vec<Key>,pub prefetch:Vec<Key>}
/// Non-wrapping tile rectangle, inclusive endpoints. Host splits dateline views.
pub fn viewport(v:View,halo:u8,max_tiles:usize)->Result<Plan,Error>{
    if v.z>30||halo>2||max_tiles==0||max_tiles>4096{return Err(Error::Invalid);}
    let size=1u32<<v.z;
    if v.min_x>v.max_x||v.min_y>v.max_y||v.max_x>=size||v.max_y>=size{return Err(Error::Invalid);}
    let count=u64::from(v.max_x-v.min_x+1)*u64::from(v.max_y-v.min_y+1);
    if count>max_tiles as u64{return Err(Error::Limit);}
    let mut visible=Vec::with_capacity(count as usize);
    for y in v.min_y..=v.max_y{for x in v.min_x..=v.max_x{visible.push(Key{z:v.z,x,y});}}
    let cx=u64::from(v.min_x)+u64::from(v.max_x);
    let cy=u64::from(v.min_y)+u64::from(v.max_y);
    let priority=|k:&Key|{let dx=(2*u64::from(k.x)).abs_diff(cx);let dy=(2*u64::from(k.y)).abs_diff(cy);(dx+dy,k.y,k.x)};
    visible.sort_by_key(priority);
    let h=u32::from(halo);
    let mut prefetch=Vec::new();
    for y in v.min_y.saturating_sub(h)..=v.max_y.saturating_add(h).min(size-1){
        for x in v.min_x.saturating_sub(h)..=v.max_x.saturating_add(h).min(size-1){
            if x>=v.min_x&&x<=v.max_x&&y>=v.min_y&&y<=v.max_y{continue;}
            prefetch.push(Key{z:v.z,x,y});
        }
    }
    prefetch.sort_by_key(priority);prefetch.truncate(max_tiles-visible.len());
    Ok(Plan{visible,prefetch})
}
#[derive(Debug,Clone)]
pub struct Grant{pub principal:String,pub project:String,pub revision:u64,pub handle:String,pub byte_len:u64,pub expires_ms:u64}
pub struct ReadRequest<'a>{pub project:&'a str,pub revision:u64,pub handle:&'a str,pub generation:u64,pub offset:u64,pub length:u64}
/// Returns half-open range; never returns a filesystem path.
pub fn authorize(g:&Grant,authenticated_principal:&str,r:ReadRequest<'_>,generation:u64,now:u64,max_read:u64)->Result<std::ops::Range<u64>,Error>{
    if !super::valid_id(r.project)||!super::valid_id(r.handle)||!super::valid_id(authenticated_principal)||
        g.principal!=authenticated_principal||g.project!=r.project||g.handle!=r.handle||now>=g.expires_ms{return Err(Error::Unauthorized);}
    if r.generation!=generation||r.revision!=g.revision{return Err(Error::Stale);}
    if r.length==0||r.length>max_read.min(8*1024*1024){return Err(Error::Limit);}
    let end=r.offset.checked_add(r.length).ok_or(Error::Overflow)?;
    if end>g.byte_len{return Err(Error::Invalid);}
    Ok(r.offset..end)
}
pub fn stale_tiles(previous:&[Key],current:&Plan)->Vec<Key>{
    let keep:BTreeSet<_>=current.visible.iter().chain(&current.prefetch).copied().collect();
    previous.iter().copied().filter(|k|!keep.contains(k)).collect::<BTreeSet<_>>().into_iter().collect()
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn visible_priority_and_prefetch_are_bounded(){
 let p=viewport(View{z:3,min_x:2,min_y:2,max_x:4,max_y:4},1,12).unwrap();
 assert_eq!(p.visible[0],Key{z:3,x:3,y:3});assert_eq!(p.visible.len(),9);assert_eq!(p.prefetch.len(),3);
 assert_eq!(stale_tiles(&[Key{z:3,x:7,y:7},p.visible[0]],&p),vec![Key{z:3,x:7,y:7}]);}
 #[test]fn rejects_oversized_view(){assert_eq!(viewport(View{z:10,min_x:0,min_y:0,max_x:100,max_y:100},1,100),Err(Error::Limit));}
 #[test]fn read_is_scoped_and_generation_checked(){
 let g=Grant{principal:"user".into(),project:"p".into(),revision:2,handle:"asset".into(),byte_len:100,expires_ms:10};
 let req=||ReadRequest{project:"p",revision:2,handle:"asset",generation:3,offset:10,length:20};
 assert_eq!(authorize(&g,"user",req(),3,1,32),Ok(10..30));
 assert_eq!(authorize(&g,"other",req(),3,1,32),Err(Error::Unauthorized));
 assert_eq!(authorize(&g,"user",req(),4,1,32),Err(Error::Stale));
 assert_eq!(authorize(&g,"user",req(),3,10,32),Err(Error::Unauthorized));}
}
