import {it,expect} from "vitest";
import {filterCrs} from "@openvietgis/ui-react/gis/model";
import {DISPLAY_CRS} from "../crs-display";
it("shared CRS search keeps Vietnamese diacritic and EPSG matching",()=>{
 const catalog=DISPLAY_CRS.map(c=>({...c,description:"EPSG:"+(c.epsg??"")}));
 expect(filterCrs(catalog,"binh phuoc").some(c=>c.id==="VN2000-TM3-106.25")).toBe(true);
 expect(filterCrs(catalog,"EPSG:9212").some(c=>c.epsg===9212)).toBe(true);
 expect(filterCrs(catalog,"not-a-real-crs")).toHaveLength(0);
});