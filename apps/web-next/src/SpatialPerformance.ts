/** Per-workspace, bounded, memory-only diagnostics. Never stores URLs, IDs or attributes. */
export type MetricKind="open"|"sync"|"render";
export type Metric={kind:MetricKind;ms:number;at:number;pages:number;cacheHits:number;cacheMisses:number;cacheBytes:number;invalidations:number;deltaLayers:number;tiles:number;ok:boolean};
export type MetricCounters={pages:number;cacheHits:number;cacheMisses:number;cacheBytes:number;invalidations:number;deltaLayers:number;tiles:number};
export const emptyCounters=():MetricCounters=>({pages:0,cacheHits:0,cacheMisses:0,cacheBytes:0,invalidations:0,deltaLayers:0,tiles:0});
export class SpatialPerformance{
  private samples:Metric[]=[];
  private clock:()=>number;
  constructor(clock:()=>number=()=>globalThis.performance.now()){this.clock=clock}
  start(kind:MetricKind){
    const started=this.clock();
    const counters=emptyCounters();
    let finished=false;
    return {counters,finish:(ok=true)=>{
      if(finished)return;finished=true;
      this.samples.push({kind,ms:Math.max(0,this.clock()-started),at:Date.now(),...counters,ok});
      if(this.samples.length>60)this.samples.shift();
    }};
  }
  snapshot(){return this.samples.map(m=>({...m}));}
  summary(kind:MetricKind){
    const all=this.samples.filter(m=>m.kind===kind),ok=all.filter(m=>m.ok),ordered=ok.map(m=>m.ms).sort((a,b)=>a-b);
    const percentile=(p:number)=>ordered.length?ordered[Math.ceil(ordered.length*p)-1]:null;
    const totals=ok.reduce((a,m)=>({cacheHits:a.cacheHits+m.cacheHits,cacheMisses:a.cacheMisses+m.cacheMisses,cacheBytes:a.cacheBytes+m.cacheBytes,invalidations:a.invalidations+m.invalidations}),{cacheHits:0,cacheMisses:0,cacheBytes:0,invalidations:0});
    const eligible=totals.cacheHits+totals.cacheMisses;
    return {count:all.length,failures:all.length-ok.length,p50:percentile(.5),p95:percentile(.95),p99:percentile(.99),cacheHitRate:eligible?totals.cacheHits/eligible:null,...totals,last:all.at(-1)??null};
  }
}
