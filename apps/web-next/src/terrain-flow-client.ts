export type TerrainFlowHandlers<T> = {
  current():Worker|null;
  clear(worker:Worker):void;
  busy(value:boolean):void;
  success(frames:T):void;
  failure(message:string):void;
};

/**
 * Bind one hydraulic worker generation. A result is accepted only while the
 * exact worker remains current; replaced/terminated generations are ignored.
 */
export function bindTerrainFlowWorker<T>(
  worker:Worker,
  handlers:TerrainFlowHandlers<T>,
){
  let settled=false;
  const finish=()=>{
    if(settled||handlers.current()!==worker)return false;
    settled=true;
    worker.terminate();
    handlers.clear(worker);
    handlers.busy(false);
    return true;
  };
  worker.onmessage=(event:MessageEvent<unknown>)=>{
    if(!finish())return;
    const data=event.data as {frames?:T;error?:string};
    if(data.frames!==undefined)handlers.success(data.frames);
    else handlers.failure(data.error??"Mô phỏng thất bại.");
  };
  worker.onerror=()=>{
    if(!finish())return;
    handlers.failure("Không khởi động được mô phỏng thủy lực.");
  };
  return {
    cancel(){
      if(settled||handlers.current()!==worker)return false;
      settled=true;
      worker.terminate();
      handlers.clear(worker);
      handlers.busy(false);
      return true;
    },
  };
}
