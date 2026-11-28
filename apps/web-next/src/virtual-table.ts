export function virtualTableRange(count:number,scroll:number,height:number,rowHeight=30,overscan=5){
 const start=Math.min(count,Math.max(0,Math.floor(Math.max(0,scroll)/rowHeight)-overscan));
 const end=Math.min(count,Math.max(start,Math.ceil((Math.max(0,scroll)+Math.max(height,rowHeight))/rowHeight)+overscan));
 return {start,end,top:start*rowHeight,bottom:(count-end)*rowHeight};
}
