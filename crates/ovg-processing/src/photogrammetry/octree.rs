//! Bounded integer voxel octree subdivision. Geometry and CRS remain host-owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block { pub origin:[u32;3], pub size:[u32;3] }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leaf { pub level:u8, pub morton:u64, pub block:Block }
#[derive(Debug, Clone, Copy)]
pub struct Policy { pub max_side:u32,pub min_side:u32,pub max_voxels:u64,pub max_leaves:usize,pub max_depth:u8 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error { Invalid, Budget, Limit }
impl Block {
    fn valid(self)->bool{(0..3).all(|i|self.size[i]>0&&self.origin[i].checked_add(self.size[i]).is_some())}
    fn volume(self)->Option<u64>{self.size.into_iter().try_fold(1u64,|n,v|n.checked_mul(u64::from(v)))}
}
pub fn split(root:Block,p:Policy)->Result<Vec<Leaf>,Error>{
    if !root.valid()||p.min_side==0||p.max_side<p.min_side||p.max_voxels==0||
        p.max_leaves==0||p.max_leaves>1_000_000||p.max_depth>21{return Err(Error::Invalid);}
    let mut pending=vec![Leaf{level:0,morton:0,block:root}];let mut out=Vec::new();
    while let Some(n)=pending.pop(){
        if n.block.size.iter().all(|s|*s<=p.max_side)&&n.block.volume().is_some_and(|v|v<=p.max_voxels){
            out.push(n);continue;
        }
        if n.level>=p.max_depth{return Err(Error::Budget);}
        let axes=n.block.size.map(|s|s/2>=p.min_side);
        if !axes.iter().any(|a|*a){return Err(Error::Budget);}
        let count=1usize<<axes.iter().filter(|a|**a).count();
        if out.len()+pending.len()+count>p.max_leaves{return Err(Error::Limit);}
        for k in (0u64..8).rev(){
            let mut child=n.block;let mut valid=true;
            for (i,axis) in axes.iter().enumerate(){
                let upper=(k&(1<<i))!=0;
                if !axis{if upper{valid=false;break;}continue;}
                let low=n.block.size[i]/2;
                if upper{child.origin[i]+=low;child.size[i]-=low;}else{child.size[i]=low;}
            }
            if valid{pending.push(Leaf{level:n.level+1,morton:(n.morton<<3)|k,block:child});}
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests{
    use super::*;
    fn p()->Policy{Policy{max_side:4,min_side:1,max_voxels:64,max_leaves:128,max_depth:10}}
    #[test] fn odd_coverage_without_overlap(){
        let root=Block{origin:[1,2,3],size:[9,7,5]};let leaves=split(root,p()).unwrap();
        let mut vox=std::collections::BTreeSet::new();
        for l in leaves{let b=l.block;for x in b.origin[0]..b.origin[0]+b.size[0]{
            for y in b.origin[1]..b.origin[1]+b.size[1]{for z in b.origin[2]..b.origin[2]+b.size[2]{assert!(vox.insert((x,y,z)));}}}}
        assert_eq!(vox.len(),9*7*5);
    }
    #[test] fn slender_blocks_and_caps(){
        let b=Block{origin:[0;3],size:[1,1,100]};assert_eq!(split(b,p()).unwrap().iter().map(|l|l.block.size[2]).sum::<u32>(),100);
        let mut policy=p();policy.max_leaves=1;assert_eq!(split(b,policy),Err(Error::Limit));
    }
}
