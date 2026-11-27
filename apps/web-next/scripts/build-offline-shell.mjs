// Generate a versioned static-shell-only worker from the exact production build.
// No API responses, auth requests, map providers, or private data enter Cache Storage.
import { chmod, readFile, readdir, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { resolve } from "node:path";

const dist = resolve("dist");
const assetNames = (await readdir(resolve(dist, "assets"))).sort().map(name => "/assets/" + name);
if (!assetNames.length) throw new Error("Offline shell requires built assets");
const index = await readFile(resolve(dist, "index.html"));
const version = createHash("sha256").update(index).update(assetNames.join("\n")).digest("hex").slice(0,24);
const shell = JSON.stringify(["/index.html", ...assetNames]);
await writeFile(resolve(dist, "offline-shell-sw.js"), `/* generated static shell: ${version} */
const CACHE = "ovg-offline-shell-${version}";
const STATIC = new Set(${shell});
self.addEventListener("install", event => event.waitUntil((async () => {
  const c = await caches.open(CACHE);
  await c.addAll([...STATIC].map(url => new Request(url, {cache:"reload",credentials:"omit"})));
  await self.skipWaiting();
})()));
self.addEventListener("activate", event => event.waitUntil((async () => {
  for (const key of await caches.keys()) if (key.startsWith("ovg-offline-shell-") && key !== CACHE) await caches.delete(key);
  await self.clients.claim();
})()));
self.addEventListener("fetch", event => {
  const req = event.request;
  const url = new URL(req.url);
  if (req.method !== "GET" || url.origin !== self.location.origin) return;
  // Only the dedicated offline route falls back to the generic static shell.
  if (req.mode === "navigate" && /^\\/offline-field\\/[^/]+\\/[^/]+\\/?$/.test(url.pathname)) {
    event.respondWith(fetch(req).catch(async () => (await caches.open(CACHE)).match("/index.html")));
  } else if (STATIC.has(url.pathname) && url.pathname !== "/index.html") {
    event.respondWith((async () => (await caches.open(CACHE)).match(url.pathname).then(cached => cached || fetch(req)))());
  }
});
`);
await chmod(resolve(dist, "offline-shell-sw.js"), 0o644);
console.log("Offline static shell generated for build", version);
