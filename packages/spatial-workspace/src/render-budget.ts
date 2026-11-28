export type RenderBudget={cacheBytes:number;overflowBytes:number;movingError:number;idleError:number;globeTiles:number};
export function renderBudget(cores:number,memoryGiB?:number):RenderBudget{
 const low=(Number.isFinite(cores)&&cores<=4)||(memoryGiB!==undefined&&Number.isFinite(memoryGiB)&&memoryGiB<=4);
 const high=Number.isFinite(cores)&&cores>=12&&memoryGiB!==undefined&&memoryGiB>=8;
 const mb=low?32:high?128:64;
 return {cacheBytes:mb*1048576,overflowBytes:mb*1048576,movingError:low?48:32,idleError:low?28:16,globeTiles:low?80:high?240:160};
}
