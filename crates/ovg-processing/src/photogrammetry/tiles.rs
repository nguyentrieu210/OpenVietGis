//! Integer pixel-grid quadtree partitioning, with separate core and halo windows.
//! Not a substitute for global camera adjustment or georeferencing.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Layer { Image, Feature, GeometryCluster, Depth, SpatialBlock, Compute, Storage, Display }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window { pub x:u32, pub y:u32, pub width:u32, pub height:u32 }
impl Window {
    fn end(self)->Option<(u32,u32)> {
        if self.width==0 || self.height==0 {return None;}
        Some((self.x.checked_add(self.width)?,self.y.checked_add(self.height)?))
    }
    fn contains(self,b:Self)->bool {
        match (self.end(),b.end()) {
            (Some((x,y)),Some((bx,by)))=>b.x>=self.x && b.y>=self.y && bx<=x && by<=y,
            _=>false,
        }
    }
    fn intersects(self,b:Self)->bool {
        match (self.end(),b.end()) {
            (Some((x,y)),Some((bx,by)))=>self.x<bx && b.x<x && self.y<by && b.y<y,
            _=>false,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TileId { pub layer:Layer, pub level:u8, pub morton:u64 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile { pub id:TileId, pub core:Window, pub halo:Window }
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    pub min_side:u32, pub max_side:u32, pub halo_px:u32, pub bytes_per_pixel:u32,
    pub max_working_bytes:u64, pub max_tiles:usize, pub max_depth:u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error { InvalidGrid, InvalidPolicy, BudgetImpossible, TileLimit, Overflow, InvalidIndex }
fn expanded(core:Window, extent:Window, halo:u32)->Result<Window,Error> {
    let (ex,ey)=extent.end().ok_or(Error::InvalidGrid)?;
    let (cx,cy)=core.end().ok_or(Error::InvalidGrid)?;
    if !extent.contains(core) {return Err(Error::InvalidGrid);}
    let x=core.x.saturating_sub(halo).max(extent.x);
    let y=core.y.saturating_sub(halo).max(extent.y);
    let r=cx.saturating_add(halo).min(ex);
    let b=cy.saturating_add(halo).min(ey);
    Ok(Window{x,y,width:r-x,height:b-y})
}
pub fn partition(extent:Window, layer:Layer, p:Policy)->Result<Vec<Tile>,Error> {
    if extent.end().is_none(){return Err(Error::InvalidGrid);}
    if p.min_side==0 || p.max_side<p.min_side || p.bytes_per_pixel==0 ||
        p.max_working_bytes==0 || p.max_tiles==0 || p.max_tiles>1_000_000 || p.max_depth>31 {
        return Err(Error::InvalidPolicy);
    }
    let mut pending=vec![(TileId{layer,level:0,morton:0},extent)];
    let mut out=Vec::new();
    while let Some((id,w))=pending.pop() {
        let h=expanded(w,extent,p.halo_px)?;
        let bytes=u64::from(h.width).checked_mul(u64::from(h.height))
            .and_then(|n|n.checked_mul(u64::from(p.bytes_per_pixel)));
        if w.width<=p.max_side && w.height<=p.max_side && bytes.is_some_and(|n|n<=p.max_working_bytes) {
            out.push(Tile{id,core:w,halo:h});continue;
        }
        if id.level>=p.max_depth {return Err(Error::BudgetImpossible);}
        let sx=w.width/2>=p.min_side; let sy=w.height/2>=p.min_side;
        if !sx && !sy {return Err(Error::BudgetImpossible);}
        let a=if sx {w.width/2} else {w.width};
        let b=if sy {w.height/2} else {w.height};
        let children=[
            Window{x:w.x,y:w.y,width:a,height:b},
            Window{x:w.x+a,y:w.y,width:w.width-a,height:b},
            Window{x:w.x,y:w.y+b,width:a,height:w.height-b},
            Window{x:w.x+a,y:w.y+b,width:w.width-a,height:w.height-b},
        ];
        let count=children.iter().filter(|c|c.width>0 && c.height>0).count();
        if out.len()+pending.len()+count>p.max_tiles {return Err(Error::TileLimit);}
        for (i,c) in children.into_iter().enumerate().rev() {
            if c.width==0 || c.height==0 {continue;}
            pending.push((TileId{layer,level:id.level+1,morton:(id.morton<<2)|i as u64},c));
        }
    }
    Ok(out)
}
/// Invalidate tiles whose halo intersects a changed pixel region. Global camera
/// graph changes require an additional dependency invalidation, not just this test.
pub fn affected(tiles:&[Tile], changed:Window)->Result<Vec<TileId>,Error>{
    if changed.end().is_none(){return Err(Error::InvalidGrid);}
    let mut seen=BTreeSet::new();
    let mut hits=Vec::new();
    for t in tiles {
        if !t.halo.contains(t.core) || !seen.insert(t.id) {return Err(Error::InvalidIndex);}
        if t.halo.intersects(changed){hits.push(t.id);}
    }
    hits.sort_unstable();Ok(hits)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn extent()->Window {Window{x:10,y:20,width:101,height:77}}
    fn policy()->Policy {Policy{min_side:8,max_side:32,halo_px:4,bytes_per_pixel:16,
        max_working_bytes:32_768,max_tiles:128,max_depth:8}}
    #[test] fn exact_odd_grid_coverage_and_halo() {
        let e=extent();let ts=partition(e,Layer::Compute,policy()).unwrap();
        let mut covered=BTreeSet::new();
        for t in &ts {
            assert!(e.contains(t.halo));assert!(t.halo.contains(t.core));
            for y in t.core.y..t.core.y+t.core.height {
                for x in t.core.x..t.core.x+t.core.width {assert!(covered.insert((x,y)));}
            }
        }
        assert_eq!(covered.len(),101*77);
    }
    #[test] fn stable_ids_and_neighbor_invalidation() {
        let ts=partition(extent(),Layer::Compute,policy()).unwrap();
        assert_eq!(ts,partition(extent(),Layer::Compute,policy()).unwrap());
        let changed=Window{x:35,y:40,width:1,height:1};
        let hits=affected(&ts,changed).unwrap();
        assert!(!hits.is_empty());assert!(hits.len()<ts.len());
    }
    #[test] fn strict_budget_and_tile_limit() {
        let mut p=policy();p.max_working_bytes=1;
        assert_eq!(partition(extent(),Layer::Compute,p),Err(Error::BudgetImpossible));
        p=policy();p.max_tiles=1;
        assert_eq!(partition(extent(),Layer::Compute,p),Err(Error::TileLimit));
    }
    #[test] fn elongated_grid_and_overflow() {
        let e=Window{x:0,y:0,width:1,height:200};
        let ts=partition(e,Layer::Image,policy()).unwrap();
        assert_eq!(ts.iter().map(|t|t.core.height).sum::<u32>(),200);
        assert_eq!(partition(Window{x:u32::MAX,y:0,width:1,height:1},Layer::Compute,policy()),Err(Error::InvalidGrid));
    }
}
