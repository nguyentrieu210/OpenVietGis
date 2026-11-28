// Only the binary fixture loader in laz-real.test.ts uses this Node API.
// Keep Node globals out of the browser application type environment.
declare module "node:fs" {
  export function readFileSync(path: URL): Uint8Array;
}
