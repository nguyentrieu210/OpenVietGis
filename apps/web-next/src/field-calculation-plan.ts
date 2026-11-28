import {parseExpression,evaluateExpression} from "./label-expr";
import {attributeValue,type AttributeField} from "./attribute-value";
import {calculateExact} from "./precise-calculation";
type Field=AttributeField&{field_id:string};
type Row={feature_id:string;attributes:Record<string,unknown>};
export type CalculationMode="ast"|"exact";
export function planFieldCalculation(fields:readonly Field[],targetId:string,rows:readonly Row[],formula:string,mode:CalculationMode){
 const target=fields.find(f=>f.field_id===targetId);
 if(!target||!rows.length||rows.length>2000||new Set(rows.map(r=>r.feature_id)).size!==rows.length||rows.some(r=>!r.feature_id))throw Error("Trường hoặc tập đối tượng tính không hợp lệ.");
 if(mode==="exact"&&target.type!=="integer"&&target.type!=="decimal")throw Error("Công thức chính xác chỉ dành cho trường số.");
 const ast=mode==="ast"?parseExpression(formula,fields.map(f=>f.field_id)):null;
 const numeric=new Set(fields.filter(f=>f.type==="integer"||f.type==="decimal").map(f=>f.field_id));
 return rows.map(row=>{
  try{
   let value:string|number|boolean|null;
   if(ast){
    const result=evaluateExpression(ast,row.attributes);
    if(result===null)value=attributeValue(target,"",true);
    else {
     if(target.type==="integer"&&(typeof result!=="number"||!Number.isSafeInteger(result)))throw Error("Kết quả cần là số nguyên an toàn.");
     if(target.type==="decimal"&&!(typeof result==="string"||typeof result==="number"&&Number.isSafeInteger(result)))throw Error("Dùng chế độ chính xác cho phép tính thập phân, hoặc trả chuỗi thập phân.");
     if(target.type==="boolean"&&typeof result!=="boolean")throw Error("Kết quả cần Boolean.");
     if((target.type==="text"||target.type==="timestamp")&&typeof result!=="string")throw Error("Kết quả cần chuỗi.");
     if(typeof result==="string"&&result.includes("\0"))throw Error("Chuỗi chứa ký tự NUL.");
     value=attributeValue(target,String(result));
    }
   }else{
    const kind=target.type==="integer"?"integer":"decimal";
    value=attributeValue(target,calculateExact(formula,row.attributes,numeric,kind));
   }
   return {feature_id:row.feature_id,old:row.attributes[targetId]??null,value,properties:{...row.attributes,[targetId]:value}};
  }catch(e){throw Error(row.feature_id+": "+(e instanceof Error?e.message:String(e)))}
 });
}
