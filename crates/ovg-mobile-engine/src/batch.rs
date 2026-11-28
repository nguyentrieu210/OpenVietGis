//! Owned WKB batches; bounded by bytes and feature count, not a zero-copy claim.
use crate::{Error, MAX_BYTES, MAX_ROWS};
use ovg_spatial::{parse_wkb, validate_geometry, ValidationLimits};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchHeader {
    pub schema: u16,
    pub workspace: String,
    pub source: String,
    pub layer: String,
    pub revision: String,
    pub epsg: u32,
    pub epoch: u64,
    pub continuation: Option<String>,
}
pub struct WkbBatch { pub header: BatchHeader, bytes: Vec<u8>, offsets: Vec<u32> }
impl WkbBatch {
    pub fn new(header: BatchHeader, bytes: Vec<u8>, offsets: Vec<u32>) -> Result<Self, Error> {
        if header.schema != 1 || header.epsg == 0
            || [&header.workspace,&header.source,&header.layer,&header.revision].iter().any(|s| s.is_empty() || s.len()>256)
            || header.continuation.as_ref().is_some_and(|s| s.len()>256)
            || bytes.len()>MAX_BYTES || offsets.len()<2 || offsets.len()>MAX_ROWS+1
            || offsets.first()!=Some(&0) || offsets.last().copied()!=Some(bytes.len() as u32)
            || offsets.windows(2).any(|w| w[0]>=w[1]) { return Err(Error::InvalidBatch); }
        for w in offsets.windows(2) {
            let geom=parse_wkb(&bytes[w[0] as usize..w[1] as usize],&ValidationLimits::default()).map_err(|_|Error::InvalidBatch)?;
            if geom.srid.is_some_and(|s|s!=header.epsg) { return Err(Error::InvalidBatch); }
            validate_geometry(&geom.geometry,&ValidationLimits::default()).map_err(|_|Error::InvalidBatch)?;
        }
        Ok(Self{header,bytes,offsets})
    }
    pub fn rows(&self)->usize { self.offsets.len()-1 }
    pub fn into_buffers(self)->(Vec<u8>,Vec<u32>){(self.bytes,self.offsets)}
}
