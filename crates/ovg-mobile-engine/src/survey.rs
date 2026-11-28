//! Explicit fix quality; no invented RTK precision.
use crate::Error;
use serde::{Serialize,Deserialize};
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
pub struct Fix{pub lon:f64,pub lat:f64,pub accuracy_m:Option<f64>,pub altitude_m:Option<f64>,pub utc_ms:i64,pub provider:String,pub quality:String,pub satellites:Option<u8>}
pub fn validate_fix(f:&Fix)->Result<(),Error>{
 if !f.lon.is_finite()||!f.lat.is_finite()||f.lon.abs()>180.||f.lat.abs()>90.||f.utc_ms<=0||f.accuracy_m.is_some_and(|a|!a.is_finite()||a<=0.)||f.altitude_m.is_some_and(|a|!a.is_finite())||!matches!(f.provider.as_str(),"os"|"nmea")||!matches!(f.quality.as_str(),"unknown"|"standalone"|"differential"|"rtk_fixed"|"rtk_float"){return Err(Error::InvalidBatch);}
 if f.provider=="os"&&f.quality!="unknown"{return Err(Error::InvalidBatch);}Ok(())
}
fn angle(s:&str,hem:&str,lat:bool)->Result<f64,Error>{
 let value=s.parse::<f64>().map_err(|_|Error::InvalidBatch)?;
 if !value.is_finite()||value<0.{return Err(Error::InvalidBatch);}
 let deg=(value/100.).floor();let min=value-deg*100.;
 if min>=60.||deg>if lat{90.}else{180.}{return Err(Error::InvalidBatch);}
 let sign=match(hem,lat){("N",true)|("E",false)=>1.,("S",true)|("W",false)=>-1.,_=>return Err(Error::InvalidBatch)};
 Ok(sign*(deg+min/60.))
}
/// GGA only. The caller supplies a trusted UTC date/time; GGA contains time of day only.
/// HDOP is not metres and is never reported as horizontal accuracy.
pub fn parse_gga(line:&str,utc_ms:i64)->Result<Fix,Error>{
 if line.len()>128||!line.is_ascii(){return Err(Error::InvalidBatch);}
 let line=line.trim_end_matches(['\r','\n']);
 let body=line.strip_prefix('$').ok_or(Error::InvalidBatch)?;
 let (data,checksum)=body.split_once('*').ok_or(Error::InvalidBatch)?;
 if checksum.len()!=2||u8::from_str_radix(checksum,16).ok()!=Some(data.bytes().fold(0,|a,b|a^b)){return Err(Error::InvalidBatch);}
 let f:Vec<&str>=data.split(',').collect();
 if f.len()<13||!matches!(f[0],"GPGGA"|"GNGGA"|"GLGGA"|"GAGGA"|"GBGGA"){return Err(Error::InvalidBatch);}
 let quality=match f[6]{"1"=>"standalone","2"=>"differential","4"=>"rtk_fixed","5"=>"rtk_float",_=>return Err(Error::InvalidBatch)};
 let satellites=f[7].parse::<u8>().map_err(|_|Error::InvalidBatch)?;
 if satellites==0||f[10]!="M"{return Err(Error::InvalidBatch);}
 let t=f[1].parse::<f64>().map_err(|_|Error::InvalidBatch)?;
 if !t.is_finite()||t<0.||t>=240000.||(t/100.).floor()%100.>=60.||t%100.>=60.{return Err(Error::InvalidBatch);}
 let fix=Fix{lon:angle(f[4],f[5],false)?,lat:angle(f[2],f[3],true)?,accuracy_m:None,altitude_m:Some(f[9].parse().map_err(|_|Error::InvalidBatch)?),utc_ms,provider:"nmea".into(),quality:quality.into(),satellites:Some(satellites)};
 validate_fix(&fix)?;Ok(fix)
}
