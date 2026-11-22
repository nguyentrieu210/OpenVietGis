/* OpenVietGIS Partner SDK 1.0 — classic script, no token or network access. */
(function (global) {
  "use strict";
  const protocol = "openvietgis.partner.v1";
  function connect({hostOrigin = "https://openvietgis.cloud", timeoutMs = 10000} = {}) {
    if (global.parent === global) return Promise.reject(new Error("Open plugin inside GIS Studio"));
    if (new URL(hostOrigin).origin !== hostOrigin) return Promise.reject(new Error("Invalid host origin"));
    let session = "", capabilities = [], counter = 0, closed = false, resolved = false;
    const pending = new Map();
    const mapMethods = ["map_view","map_navigate","map_selection","map_overlay","tool_register"];
    const toolListeners=new Set();
    let resolveReady, rejectReady;
    const ready = new Promise((resolve, reject) => { resolveReady = resolve; rejectReady = reject; });
    const send = (data, transfer = []) => global.parent.postMessage({protocol, ...data}, hostOrigin, transfer);
    const hello = () => send({type:"hello"});
    const failPending = (reason) => {
      for (const p of pending.values()) { clearTimeout(p.timer); p.port?.close(); p.reject(new Error(reason)); }
      pending.clear();
    };
    const close = () => {
      if (closed) return;
      closed = true;
      clearInterval(retry); clearTimeout(deadline);
      global.removeEventListener("message", receive);
      failPending("SDK disconnected");toolListeners.clear();
      if (!resolved) rejectReady(new Error("SDK disconnected"));
    };
    const request = (method, params = {}) => {
      if (closed || !session || !capabilities.includes(method)) return Promise.reject(new Error("Capability unavailable"));
      const id = String(++counter);
      return new Promise((resolve, reject) => {
        const channel=mapMethods.includes(method)?new MessageChannel():null;
        const timer = setTimeout(() => { pending.delete(id); channel?.port1.close(); reject(new Error("SDK request timeout")); }, channel?Math.max(45000,timeoutMs):timeoutMs);
        pending.set(id, {resolve, reject, timer,port:channel?.port1});
        if(channel){
          channel.port1.onmessage=(event)=>{
            const d=event.data;
            if(!pending.has(id)||!d||d.protocol!==protocol||d.type!=="map_response")return;
            pending.delete(id);clearTimeout(timer);channel.port1.close();
            if(d.ok===true)resolve(d.result);else reject(new Error(String(d.result)));
          };
        }
        send({type:"request", session, id, method, params},channel?[channel.port2]:[]);
      });
    };
    const api = Object.freeze({
      get capabilities() { return [...capabilities]; },
      getContext: () => request("host_context"),
      notice: (text) => request("host_notice", {text}),
      getMapView: () => request("map_view"),
      getSelection: () => request("map_selection"),
      navigateMap: (target) => request("map_navigate", target),
      setMapOverlay: (overlay) => request("map_overlay", overlay),
      registerTool: (tool) => request("tool_register",tool),
      onToolClick: (listener) => {if(typeof listener!=="function")throw new TypeError("Listener required");toolListeners.add(listener);return ()=>toolListeners.delete(listener)},
      close,
    });
    function receive(event) {
      if (event.source !== global.parent || event.origin !== hostOrigin) return;
      const d = event.data;
      if (!d || d.protocol !== protocol) return;
      if (d.type === "reset") { session = ""; failPending("SDK session changed"); hello(); return; }
      if (d.type === "welcome" && typeof d.session === "string" && Array.isArray(d.capabilities)) {
        if (session && session !== d.session) failPending("SDK session changed");
        session = d.session; capabilities = d.capabilities.filter(c => c === "host_context" || c === "host_notice" || mapMethods.includes(c));
        if (!resolved) { resolved = true; clearTimeout(deadline); resolveReady(api); }
        return;
      }
      if(d.type==="tool_click"&&d.session===session&&session&&capabilities.includes("tool_register")&&typeof d.toolId==="string"){for(const listener of [...toolListeners]){try{listener(d.toolId)}catch(e){console.error(e)}}return}
      if (d.type !== "response" || d.session !== session || !pending.has(d.id)) return;
      const p = pending.get(d.id); if(p.port)return; pending.delete(d.id); clearTimeout(p.timer);
      if (d.ok === true) p.resolve(d.result); else p.reject(new Error(String(d.result)));
    }
    global.addEventListener("message", receive);
    const retry = setInterval(() => { if (!session) hello(); }, 300);
    const deadline = setTimeout(close, timeoutMs);
    // Let the parent iframe load event establish the session first.
    setTimeout(hello, 100);
    return ready;
  }
  global.OpenVietGISPartner = Object.freeze({connect});
})(window);
