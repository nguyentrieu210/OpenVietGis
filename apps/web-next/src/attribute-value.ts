// Canonical attribute values compatible with the Rust spatial schema.
export type AttributeField={name:string;type:string;nullable:boolean};
export function attributeValue(f:AttributeField,raw:string,isNull=false):string|number|boolean|null{
 const bad=(reason:string):never=>{throw Error(f.name+": "+reason)};
 if(isNull)return f.nullable?null:bad("không cho phép NULL");
 if(f.type==="text")return raw;
 if(f.type==="integer"){const n=Number(raw);return /^-?\d+$/.test(raw)&&Number.isSafeInteger(n)&&n>=-2147483648&&n<=2147483647?n:bad("số nguyên 32 bit không hợp lệ")}
 if(f.type==="decimal"){
  if(!/^[+-]?\d+(\.\d+)?$/.test(raw))return bad("nhập số thập phân bằng dấu chấm");
  const [whole,frac=""]=raw.replace(/^[+-]/,"").split("."),i=whole!.replace(/^0+/,"")||"0",d=frac.replace(/0+$/,"");
  if((i==="0"?0:i.length)+d.length>38||d.length>18)return bad("tối đa 38 chữ số, 18 chữ số thập phân");
  return (raw.startsWith("-")&&(i!=="0"||d)?"-":"")+i+(d?"."+d:"");
 }
 if(f.type==="boolean")return raw==="true"?true:raw==="false"?false:bad("chọn true hoặc false");
 if(f.type==="timestamp"){
  const m=/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(Z|[+-]\d{2}:\d{2})$/.exec(raw);
  if(!m)return bad("nhập RFC3339 có múi giờ");
  const y=Number(m[1]),mo=Number(m[2]),d=Number(m[3]),hh=Number(m[4]),mm=Number(m[5]),ss=Number(m[6]),leap=y%4===0&&(y%100!==0||y%400===0),days=[31,leap?29:28,31,30,31,30,31,31,30,31,30,31];
  if(mo<1||mo>12||d<1||d>days[mo-1]!||hh>23||mm>59||ss>59||!Number.isFinite(Date.parse(raw)))return bad("ngày giờ không hợp lệ");
  return raw;
 }
 return bad("kiểu dữ liệu chưa hỗ trợ");
}
