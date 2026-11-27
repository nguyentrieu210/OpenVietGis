import { describe, expect, it } from "vitest";
import { availableOperators, buildQueryDefinition, type FilterRow, type QueryField } from "./query-filters";

const fields: QueryField[] = [
  {field_id:"name",name:"Name",ordinal:0,type:"text",nullable:true},
  {field_id:"count",name:"Count",ordinal:1,type:"integer",nullable:false},
  {field_id:"measure",name:"Measure",ordinal:2,type:"decimal",nullable:false},
  {field_id:"visible",name:"Visible",ordinal:3,type:"boolean",nullable:true},
  {field_id:"when",name:"When",ordinal:4,type:"timestamp",nullable:false}
];
const row = (field:string,value:string,op:FilterRow["op"]="eq",negate=false):FilterRow=>({field,value,op,negate});
const definition = (rows:FilterRow[],group:"and"|"or"="and")=>buildQueryDefinition(rows,fields,group,"intersects");

describe("typed R007 saved query definition",()=>{
  it("composes typed AND/NOT conditions without pagination or pin fields",()=>{
    expect(definition([row("count","12","gte"),row("visible","false","eq",true)])).toEqual({
      attribute_filter:{op:"and",conditions:[{field:"count",op:"gte",value:12},{op:"not",condition:{field:"visible",op:"eq",value:false}}]}
    });
  });
  it("uses literal empty text, typed null, and OR",()=>{
    expect(definition([row("name",""),row("name","","is_not_null")],"or")).toEqual({
      attribute_filter:{op:"or",conditions:[{field:"name",op:"eq",value:""},{field:"name",op:"is_null",value:false}]}
    });
  });
  it("canonicalizes exact decimals without conversion through Number",()=>{
    expect(definition([row("measure","00012345678901234567890.1234500")])).toEqual({
      attribute_filter:{field:"measure",op:"eq",value:"12345678901234567890.12345"}
    });
    expect(definition([row("measure","-0.000")])).toEqual({attribute_filter:{field:"measure",op:"eq",value:"0"}});
  });
  it.each([row("count","1.25"),row("count","2147483648"),row("visible","0"),row("measure","1e9"),row("measure","0.1234567890123456789"),row("when","2026-11-21"),row("count","","is_null"),row("visible","true","gt"),row("not_in_schema","x")])("rejects invalid typed operands and fields %j",(invalid)=>{
    expect(()=>definition([invalid])).toThrow();
  });
  it("supports an unfiltered query and a reference-layer relation",()=>{
    expect(definition([row("","")])).toEqual({});
    expect(buildQueryDefinition([row("","")],fields,"and","within","ref")).toEqual({spatial_filter:{predicate:"within",reference_layer_id:"ref"}});
    expect(availableOperators(fields[3])).not.toContain("contains");
  });
  it("rejects a partially specified group",()=>expect(()=>definition([row("name","X"),row("","")])).toThrow());
});
