"use strict";
document.querySelector("#run").addEventListener("click", async () => {
  const output = document.querySelector("#output");
  const result = { webgl2: !!document.createElement("canvas").getContext("webgl2"), webgpu: !!navigator.gpu };
  for (const url of ["ovg://localhost/probe", "http://ovg.localhost/probe"]) {
    try {
      const response = await fetch(url, {cache:"no-store"});
      const bytes = Array.from(new Uint8Array(await response.arrayBuffer()));
      result[url] = {status: response.status, bytes, pass: response.ok && bytes.join(",")==="79,86,71,1"};
    } catch (error) { result[url] = {pass:false, error:String(error)}; }
  }
  output.textContent = JSON.stringify(result, null, 2);
});
