import type {LayerView} from "./LayerProperties";
export function tileLayerPolicy(v?:LayerView){
 const min=v?.minZoom??0,max=v?.maxZoom??22;
 const unsupported=!!(v?.filterField||v?.hiddenCats?.length||v?.renderer==="categorized"||v?.renderer==="graduated"||v?.renderer==="rules"||Object.values(v?.dataDefined??{}).some(Boolean));
 return {min,max,selectable:v?.selectable!==false,opacity:v?.opacity??1,unsupported};
}
