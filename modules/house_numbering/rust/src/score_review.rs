//! Read-only scoring preview using the thresholds of Bình Hưng e9e4f0d.
use serde::{Deserialize,Serialize};
use std::collections::{HashMap,BTreeMap};
use unicode_normalization::{UnicodeNormalization,char::is_combining_mark};

#[derive(Clone,Debug,Deserialize)]
pub struct PointRow{
    pub id:String,pub number:Option<String>,pub road:Option<String>,
    pub x:Option<f64>,pub y:Option<f64>,
}
impl PointRow{
    fn xy(&self)->Option<(f64,f64)>{Some((self.x.filter(|v|v.is_finite())?,self.y.filter(|v|v.is_finite())?))}
}
fn compact(s:&str)->String{s.chars().filter(|c|!c.is_whitespace()).flat_map(char::to_uppercase).collect()}
pub fn number(s:&str)->Option<String>{
    let v=compact(s);
    let mut parts=v.split(['/', '-']);
    let first=parts.next()?;
    let valid=|p:&str|!p.is_empty()&&p.chars().all(|c|c.is_ascii_alphanumeric()||c=='Đ');
    if !valid(first)||!first.chars().any(|c|c.is_ascii_digit())||!parts.all(valid){None}else{Some(v)}
}
fn alley(s:&str)->Option<String>{
    let v=compact(s);let p=v.strip_suffix('/')?;
    if p.contains('-'){return None;}number(p)
}
pub fn road(s:&str)->String{
    let normalized:String=s.replace(['đ','Đ'],"d").nfd().filter(|c|!is_combining_mark(*c)).flat_map(char::to_lowercase).map(|c|if c.is_ascii_alphanumeric(){c}else{' '}).collect();
    let mut words:Vec<String>=normalized.split_whitespace().map(str::to_owned).collect();
    if words.first().is_some_and(|w|w=="duong"||w=="hem"){words.remove(0);}
    let mut out=Vec::new();
    for w in words{if let Some(rest)=w.strip_prefix("ql"){out.push("quoc lo".into());if !rest.is_empty(){out.push(rest.into());}}else{out.push(w)}}
    out.join(" ")
}
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="snake_case")]
pub enum Status{Khop,LechViTri,SaiSo,KhongCo,ThieuSo,ChuaViTri}
impl Status{fn rank(self)->u8{match self{Self::Khop=>0,Self::LechViTri=>1,Self::SaiSo=>2,Self::KhongCo=>3,Self::ThieuSo=>4,Self::ChuaViTri=>5}}
 pub fn key(self)->&'static str{match self{Self::Khop=>"khop",Self::LechViTri=>"lech_vi_tri",Self::SaiSo=>"sai_so",Self::KhongCo=>"khong_co",Self::ThieuSo=>"thieu_so",Self::ChuaViTri=>"chua_vi_tri"}}}
#[derive(Debug,Serialize)]
pub struct ResultRow{
    pub id:String,pub number:Option<String>,pub road:Option<String>,pub status:Status,
    pub grid_id:Option<String>,pub grid_number:Option<String>,pub distance_m:Option<f64>,pub duplicate_peers:usize,
}
struct Grid<'a>{row:&'a PointRow,number:String,road:String,xy:(f64,f64)}
fn cell(x:f64,y:f64)->(i64,i64){((x/50.).floor() as i64,(y/50.).floor() as i64)}
fn distance(a:(f64,f64),b:(f64,f64))->f64{(a.0-b.0).hypot(a.1-b.1)}
pub fn evaluate(points:&[PointRow],grid:&[PointRow])->(Vec<ResultRow>,BTreeMap<&'static str,usize>,usize){
    let grid:Vec<_>=grid.iter().filter_map(|row|Some(Grid{row,number:number(row.number.as_deref()?)?,road:road(row.road.as_deref().unwrap_or("")),xy:row.xy()?})).collect();
    let mut spatial:HashMap<(i64,i64),Vec<usize>>=HashMap::new();
    let mut by_number:HashMap<&str,Vec<usize>>=HashMap::new();
    for (i,g) in grid.iter().enumerate(){spatial.entry(cell(g.xy.0,g.xy.1)).or_default().push(i);by_number.entry(&g.number).or_default().push(i);}
    let mut duplicates:HashMap<(String,String),usize>=HashMap::new();
    for p in points{if let Some(n)=number(p.number.as_deref().unwrap_or("")){*duplicates.entry((n,road(p.road.as_deref().unwrap_or("")))).or_default()+=1;}}
    let nearest=|xy:(f64,f64),radius:f64,prefix:Option<&str>|->Option<(usize,f64)>{
        let (cx,cy)=cell(xy.0,xy.1);let k=(radius/50.).ceil() as i64;
        let mut best:Option<(usize,f64)>=None;
        for dx in -k..=k{for dy in -k..=k{for &i in spatial.get(&(cx+dx,cy+dy)).into_iter().flatten(){
            let g=&grid[i];let d=distance(xy,g.xy);
            if d<=radius&&prefix.is_none_or(|p|g.number.starts_with(p))&&best.is_none_or(|(j,old)|d<old||(d==old&&g.row.id<grid[j].row.id)){best=Some((i,d))}
        }}}
        best
    };
    let mut counts=BTreeMap::from([("khop",0),("lech_vi_tri",0),("sai_so",0),("khong_co",0),("thieu_so",0),("chua_vi_tri",0)]);
    let mut duplicate_count=0;let mut result=Vec::with_capacity(points.len());
    for p in points{
        let raw=p.number.as_deref().unwrap_or("");let street=road(p.road.as_deref().unwrap_or(""));
        let peers=number(raw).and_then(|n|duplicates.get(&(n,street.clone())).copied()).unwrap_or(1).saturating_sub(1);
        if peers>0{duplicate_count+=1;}
        let (status,hit)=if let Some(xy)=p.xy(){
            let numbers:Vec<_>=raw.split([',',';']).filter_map(number).collect();
            if numbers.is_empty(){
                let prefix=alley(raw).map(|v|v+"/");
                (Status::ThieuSo,prefix.as_deref().and_then(|v|nearest(xy,10.,Some(v))).or_else(||nearest(xy,6.,None)))
            }else{
                numbers.iter().map(|n|{
                    let same=by_number.get(n.as_str()).into_iter().flatten().filter_map(|&i|{
                        let g=&grid[i];let d=distance(xy,g.xy);let priority=if !g.road.is_empty()&&g.road==street{0}else if g.road.is_empty(){1}else{2};
                        (d<=300.).then_some((priority,d,i))
                    }).min_by(|a,b|a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(grid[a.2].row.id.cmp(&grid[b.2].row.id)));
                    if let Some((_,d,i))=same{(if d<=15.{Status::Khop}else{Status::LechViTri},Some((i,d)))}
                    else if let Some(hit)=nearest(xy,6.,None){(Status::SaiSo,Some(hit))}else{(Status::KhongCo,None)}
                }).min_by_key(|(s,_)|s.rank()).unwrap()
            }
        }else{(Status::ChuaViTri,None)};
        *counts.get_mut(status.key()).unwrap()+=1;
        result.push(ResultRow{id:p.id.clone(),number:p.number.clone(),road:p.road.clone(),status,grid_id:hit.map(|(i,_)|grid[i].row.id.clone()),grid_number:hit.and_then(|(i,_)|grid[i].row.number.clone()),distance_m:hit.map(|(_,d)|(d*10.).round()/10.),duplicate_peers:peers});
    }
    (result,counts,duplicate_count)
}
#[cfg(test)]
mod tests{
    use super::*;
    fn row(id:&str,n:&str,x:Option<f64>,y:f64)->PointRow{PointRow{id:id.into(),number:Some(n.into()),road:Some("QL50".into()),x,y:Some(y)}}
    #[test]
    fn reference_nine_points(){
        let grid=[row("g1","100",Some(100.),8.),row("g2","102",Some(120.),8.),row("g3","104",Some(140.),8.),row("g4","106",Some(160.),8.),row("g5","77/3",Some(200.),8.)];
        let points=[row("A","100",Some(101.),9.),row("B","102",Some(180.),8.),row("C","999",Some(140.5),8.),row("D","777",Some(300.),8.),row("E","721/",Some(250.),8.),row("F","5",None,0.),row("G","100",Some(102.),10.),row("H","77/",Some(204.),9.),row("I","859, 104",Some(140.),9.)];
        let (r,c,d)=evaluate(&points,&grid);assert_eq!(r.iter().map(|x|x.status).collect::<Vec<_>>(),[Status::Khop,Status::LechViTri,Status::SaiSo,Status::KhongCo,Status::ThieuSo,Status::ChuaViTri,Status::Khop,Status::ThieuSo,Status::Khop]);
        assert_eq!(c.values().sum::<usize>(),9);assert_eq!(d,2);assert_eq!(r[7].grid_number.as_deref(),Some("77/3"));assert_eq!(r[1].distance_m,Some(60.));assert_eq!(r[2].grid_number.as_deref(),Some("104"));
    }
    #[test]
    fn bounds_and_normalization(){
        assert_eq!(road("Đường Quốc lộ 50"),road("QL50"));assert_eq!(number(" 443/54/17c ").as_deref(),Some("443/54/17C"));assert_eq!(number("721/"),None);
        let grid=[row("g","100",Some(0.),0.)];
        let (r,_,_)=evaluate(&[row("a","100",Some(15.),0.),row("b","100",Some(300.),0.),row("c","100",Some(300.01),0.),row("d","999",Some(6.),0.)],&grid);
        assert_eq!(r.iter().map(|x|x.status).collect::<Vec<_>>(),[Status::Khop,Status::LechViTri,Status::KhongCo,Status::SaiSo]);
    }
}
