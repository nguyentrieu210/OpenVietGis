import {readCache,writeCache,clearSpatialCache} from "./SpatialCacheStorage";
let queue:Promise<void>=Promise.resolve();
self.onmessage=e=>{
  const {id,command}=e.data;
  queue=queue.then(async()=>{
    try{
      let value:unknown=true;
      if(command.kind==="read")value=await readCache(command.scope,command.layerId);
      else if(command.kind==="write")await writeCache(command.scope,command.layer,command.rows);
      else if(command.kind==="clear")await clearSpatialCache(command.account,command.scope);
      else value=null;
      self.postMessage({id,value});
    }catch{self.postMessage({id,value:null})}
  });
};
