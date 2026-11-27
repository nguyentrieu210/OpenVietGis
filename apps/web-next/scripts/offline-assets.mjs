// Compute the exact static dependency closure of the offline entry.
// Dynamic imports outside that entry are deliberately not precached.
export function offlineAssets(manifest) {
  const selected = new Set();
  const visited = new Set();
  const entry = Object.entries(manifest).filter(([,v])=>v?.isEntry);
  const offline = Object.keys(manifest).filter(k=>k==="src/offline-entry.tsx");
  if (entry.length !== 1 || offline.length !== 1) throw Error("Expected one SPA and one offline entry");
  function add(path) {
    if (typeof path !== "string" || !/^assets\/[a-zA-Z0-9_.@-]+$/.test(path)) throw Error("Invalid offline asset path: "+path);
    selected.add("/"+path);
  }
  function visit(key) {
    if (visited.has(key)) return;
    visited.add(key);
    const item = manifest[key];
    if (!item) throw Error("Missing offline dependency: "+key);
    add(item.file);
    for (const path of [...(item.css??[]),...(item.assets??[])]) add(path);
    for (const imported of item.imports??[]) visit(imported);
  }
  visit(entry[0][0]); visit(offline[0]);
  return [...selected].sort();
}
