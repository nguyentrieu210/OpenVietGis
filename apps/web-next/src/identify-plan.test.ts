import {describe,it,expect} from "vitest";
import {identifyHits,identifyRows,mapTipText} from "./identify-plan";
import type {StudioRow} from "./studio-contract";
const row=(layer:string,authorized=true)=>({key:"s:f",layer:{id:layer,lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:authorized,technical_supported:true}],source:{schema:{fields:[{field_id:"tip"}]}}},feature:{feature_id:"f",attributes:{tip:"<script>alert(1)</script>"}}} as unknown as StudioRow);
describe("multi-layer identify",()=>{it("keeps same FID in different layers but deduplicates symbols",()=>{const a={properties:{layerId:"a",key:"s:f"}},b={properties:{layerId:"b",key:"s:f"}};const hits=identifyHits([a,a,b]);expect(hits.length).toBe(2);expect(identifyRows(hits,[row("a"),row("b",false)]).map(r=>r.layer.id)).toEqual(["a"]);expect(mapTipText(row("a"),"tip")).toBe("<script>alert(1)</script>");expect(mapTipText(row("a"),"other")).toBe("")})});
