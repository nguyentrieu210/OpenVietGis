import {test} from 'node:test';
import assert from 'node:assert/strict';

test('crc32 matches the standard check value', async () => {
  const {crc32} = await import('./src/zip.ts');
  assert.equal(crc32(new TextEncoder().encode('123456789')), 0xcbf43926);
});

test('zip has readable entries with UTF-8 names and unique file names', async () => {
  const {zipFiles} = await import('./src/zip.ts');
  const blob = await zipFiles([
    {name: 'khao-sat.gpkg', blob: new Blob(['abc'])},
    {name: 'khao-sat.gpkg', blob: new Blob(['de'])},
    {name: 'Ấp 27.geojson', blob: new Blob(['{}'])}
  ], new Date(2026, 10, 25, 12, 30, 10));
  const bytes = new Uint8Array(await blob.arrayBuffer());
  const view = new DataView(bytes.buffer);
  const end = bytes.length - 22;
  assert.equal(view.getUint32(end, true), 0x06054b50);
  assert.equal(view.getUint16(end + 10, true), 3);
  let at = view.getUint32(end + 16, true);
  const names = [];
  for (let i = 0; i < 3; i++) {
    assert.equal(view.getUint32(at, true), 0x02014b50);
    const len = view.getUint16(at + 28, true), local = view.getUint32(at + 42, true);
    names.push(new TextDecoder().decode(bytes.subarray(at + 46, at + 46 + len)));
    assert.equal(view.getUint32(local, true), 0x04034b50);
    at += 46 + len;
  }
  assert.deepEqual(names, ['khao-sat.gpkg', 'khao-sat-2.gpkg', 'Ấp 27.geojson']);
});
