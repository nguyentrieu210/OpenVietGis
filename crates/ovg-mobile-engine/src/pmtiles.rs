//! Bounded read-only PMTiles v3 local archives. No HTTP, writes or decompressed tile decoding.
use std::{fs::File,io::{Read,Seek,SeekFrom},path::Path};
use flate2::read::GzDecoder;
use crate::Error;
const MAX:usize=4*1024*1024;
#[derive(Clone,Copy)]struct Range{offset:u64,len:u64}
#[derive(Clone,Copy)]struct Entry{id:u64,run:u64,len:u64,offset:u64}
pub struct Archive{file:File,root:Vec<Entry>,leaf:Range,tiles:Range,internal:u8,pub tile_compression:u8,pub tile_type:u8,pub attribution:String,pub min_zoom:u8,pub max_zoom:u8}
fn u64_at(h:&[u8],i:usize)->u64{u64::from_le_bytes(h[i..i+8].try_into().expect("validated header"))}
fn within(r:Range,parent:Range)->Result<Range,Error>{
    let end=r.offset.checked_add(r.len).ok_or(Error::InvalidBatch)?;
    let pe=parent.offset.checked_add(parent.len).ok_or(Error::InvalidBatch)?;
    if r.offset<parent.offset||end>pe{return Err(Error::InvalidBatch);}Ok(r)
}
fn read(file:&mut File,r:Range)->Result<Vec<u8>,Error>{
    if r.len>MAX as u64{return Err(Error::Limit);}
    let mut bytes=vec![0;r.len as usize];file.seek(SeekFrom::Start(r.offset)).map_err(|_|Error::InvalidBatch)?;
    file.read_exact(&mut bytes).map_err(|_|Error::InvalidBatch)?;Ok(bytes)
}
fn decode(data:Vec<u8>,kind:u8)->Result<Vec<u8>,Error>{
    match kind{1=>Ok(data),2=>{let mut out=Vec::new();GzDecoder::new(data.as_slice()).take((MAX+1)as u64).read_to_end(&mut out).map_err(|_|Error::InvalidBatch)?;if out.len()>MAX{return Err(Error::Limit);}Ok(out)},_=>Err(Error::InvalidBatch)}
}
fn varint(b:&[u8],i:&mut usize)->Result<u64,Error>{
    let mut n=0u64;
    for shift in (0..=63).step_by(7){let x=*b.get(*i).ok_or(Error::InvalidBatch)?;*i+=1;
        if shift==63&&x>1{return Err(Error::InvalidBatch);}
        n|=u64::from(x&127)<<shift;if x&128==0{return Ok(n);}
    }Err(Error::InvalidBatch)
}
fn directory(data:&[u8])->Result<Vec<Entry>,Error>{
    let mut i=0;let n=varint(data,&mut i)?;
    if n==0||n>65536||n>data.len()as u64/4{return Err(Error::Limit);}
    let mut v=Vec::with_capacity(n as usize);let mut id=0u64;
    for j in 0..n{let delta=varint(data,&mut i)?;if j>0&&delta==0{return Err(Error::InvalidBatch);}id=id.checked_add(delta).ok_or(Error::InvalidBatch)?;v.push(Entry{id,run:0,len:0,offset:0});}
    for e in &mut v{e.run=varint(data,&mut i)?;e.id.checked_add(e.run).ok_or(Error::InvalidBatch)?;}
    for e in &mut v{e.len=varint(data,&mut i)?;if e.len==0||e.len>MAX as u64{return Err(Error::Limit);}}
    let mut end=None;
    for e in &mut v{let x=varint(data,&mut i)?;e.offset=if x==0{end.ok_or(Error::InvalidBatch)?}else{x-1};end=Some(e.offset.checked_add(e.len).ok_or(Error::InvalidBatch)?);}
    if i!=data.len()||v.windows(2).any(|w|w[0].run>0&&w[0].id+w[0].run>w[1].id){return Err(Error::InvalidBatch);}Ok(v)
}
pub fn tile_id(z:u8,x:u32,y:u32)->Result<u64,Error>{
    if z>24{return Err(Error::InvalidBatch);}let n=1i64<<z;if i64::from(x)>=n||i64::from(y)>=n{return Err(Error::InvalidBatch);}
    let(mut x,mut y)=(i64::from(x),i64::from(y));let mut d=0i64;let mut s=n/2;
    while s>0{let rx=i64::from((x&s)>0);let ry=i64::from((y&s)>0);d+=s*s*((3*rx)^ry);
        if ry==0{if rx==1{x=s-1-x;y=s-1-y;}std::mem::swap(&mut x,&mut y);}s/=2;}
    Ok(((n*n-1)/3+d)as u64)
}
fn lookup(v:&[Entry],id:u64)->Option<Entry>{let index=v.partition_point(|e|e.id<=id).checked_sub(1)?;let e=v[index];if e.run==0||id-e.id<e.run{Some(e)}else{None}}
impl Archive{
    pub fn open(path:&Path)->Result<Self,Error>{
        if !std::fs::symlink_metadata(path).map_err(|_|Error::InvalidBatch)?.file_type().is_file(){return Err(Error::InvalidBatch);}
        let mut file=File::open(path).map_err(|_|Error::InvalidBatch)?;
        let total=file.metadata().map_err(|_|Error::InvalidBatch)?.len();
        let mut h=[0u8;127];file.read_exact(&mut h).map_err(|_|Error::InvalidBatch)?;
        if &h[..7]!=b"PMTiles"||h[7]!=3||!matches!(h[97],1|2)||!matches!(h[98],1|2)||!matches!(h[99],1..=4)||h[100]>h[101]||h[101]>24{return Err(Error::InvalidBatch);}
        let section=|i|within(Range{offset:u64_at(&h,i),len:u64_at(&h,i+8)},Range{offset:127,len:total.checked_sub(127).ok_or(Error::InvalidBatch)?});
        let root=section(8)?;let metadata=section(24)?;let leaf=section(40)?;let tiles=section(56)?;
        let ranges=[root,metadata,leaf,tiles];
        for (i,a) in ranges.iter().enumerate(){for b in &ranges[i+1..]{if a.len>0&&b.len>0&&a.offset<b.offset+b.len&&b.offset<a.offset+a.len{return Err(Error::InvalidBatch);}}}
        let root=directory(&decode(read(&mut file,root)?,h[97])?)?;
        let meta:serde_json::Value=serde_json::from_slice(&decode(read(&mut file,metadata)?,h[97])?).map_err(|_|Error::InvalidBatch)?;
        let attribution=meta["attribution"].as_str().filter(|s|!s.trim().is_empty()&&s.len()<=8192).ok_or(Error::InvalidBatch)?.to_owned();
        Ok(Self{file,root,leaf,tiles,internal:h[97],tile_compression:h[98],tile_type:h[99],attribution,min_zoom:h[100],max_zoom:h[101]})
    }
    pub fn tile(&mut self,z:u8,x:u32,y:u32)->Result<Option<Vec<u8>>,Error>{
        let id=tile_id(z,x,y)?;if z<self.min_zoom||z>self.max_zoom{return Ok(None);}
        let mut entry=lookup(&self.root,id);
        for _ in 0..4{
            let Some(e)=entry else{return Ok(None)};
            let parent=if e.run==0{self.leaf}else{self.tiles};
            let range=within(Range{offset:parent.offset.checked_add(e.offset).ok_or(Error::InvalidBatch)?,len:e.len},parent)?;
            let bytes=read(&mut self.file,range)?;
            if e.run>0{return Ok(Some(bytes));}
            entry=lookup(&directory(&decode(bytes,self.internal)?)?,id);
        }Err(Error::InvalidBatch)
    }
}
