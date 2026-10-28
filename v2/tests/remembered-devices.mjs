import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';

const source = await readFile(new URL('../src/lib/remembered-devices.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, {
  fileName: fileURLToPath(new URL('../src/lib/remembered-devices.ts', import.meta.url)),
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText;
const {
  MAX_REMEMBERED_DEVICES,
  REMEMBERED_DEVICES_KEY,
  deserializeRememberedDevices,
  serializeRememberedDevices,
  verifiedDeviceHardwareId,
  updateRememberedDevices,
  renameRememberedDevice,
  forgetRememberedDevice,
  rememberedDeviceLabel,
  readRememberedDevices,
  writeRememberedDevices,
} = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);

const firstSeen = Date.parse('2026-10-28T12:00:00Z');
const later = firstSeen + 60_000;
function device(hardwareId, overrides = {}) {
  return {
    id: 1, serial: '192.0.2.10:5555', name: 'Living room', model: 'TV model',
    device_type: 'unknown', tv_evidence: 'unknown', status: 'device', connection: 'network',
    properties: { serial_number: hardwareId, friendly_name: 'Living room', model: 'TV model' },
    ...overrides,
  };
}

let records = updateRememberedDevices([], [device(' TV-A ')], firstSeen);
assert.equal(records.length, 1);
assert.deepEqual(records[0], { hardwareId: 'TV-A', name: 'Living room', model: 'TV model', lastSeen: new Date(firstSeen).toISOString() });
assert.equal(verifiedDeviceHardwareId(device('TV-A')), 'TV-A');
assert.equal(verifiedDeviceHardwareId(device('TV-A', { status: 'offline' })), undefined);
for (const id of ['', ' ', 'unknown', 'UNKNOWN', undefined, 'a\u0000b', 'x'.repeat(257)]) {
  assert.deepEqual(updateRememberedDevices([], [device(id)], firstSeen), [], `no identity from ${JSON.stringify(id)}`);
}
assert.deepEqual(updateRememberedDevices([], [device('TV-A', { status: 'unauthorized' })], firstSeen), []);
assert.deepEqual(updateRememberedDevices([], [device('TV-A', { properties: null })], firstSeen), []);
console.log('PASS only live verified hardware identities are remembered');

const offline = device('TV-A', { status: 'offline', properties: null });
assert.deepEqual(updateRememberedDevices(records, [offline], later), records, 'unreadable offline transport cannot overwrite or refresh the saved name');
const sameAddressNewTv = updateRememberedDevices(records, [device('TV-B')], later);
assert.equal(sameAddressNewTv.length, 2, 'different IDs with the same endpoint and name remain separate');
const changedAddressSameTv = updateRememberedDevices(records, [device('TV-A', { serial: '192.0.2.99:7777' })], later);
assert.equal(changedAddressSameTv.length, 1, 'verified identity survives address changes');
assert.equal(changedAddressSameTv[0].lastSeen, new Date(later).toISOString());
const serialized = serializeRememberedDevices(sameAddressNewTv);
assert.ok(!serialized.includes('192.0.2.'), 'no endpoint is persisted for reconnect');
assert.deepEqual(Object.keys(JSON.parse(serialized).devices[0]).sort(), ['hardwareId', 'lastSeen', 'model', 'name']);
console.log('PASS historical names remain separate from unverified transports and contain no endpoint');

const renamed = renameRememberedDevice(records, 'TV-A', ' Bedroom TV ');
assert.equal(rememberedDeviceLabel(renamed[0]), 'Bedroom TV');
assert.equal(renamed[0].name, 'Living room', 'local alias does not replace the reported name');
const newReportedName = updateRememberedDevices(renamed, [device('TV-A', {
  properties: { serial_number: 'TV-A', friendly_name: 'New TV name' }, model: 'New model',
})], later);
assert.equal(newReportedName[0].name, 'New TV name');
assert.equal(newReportedName[0].alias, 'Bedroom TV');
assert.equal(newReportedName[0].model, 'New model');
assert.equal(rememberedDeviceLabel(renameRememberedDevice(newReportedName, 'TV-A', ' ')[0]), 'New TV name');
assert.equal(renameRememberedDevice(records, 'TV-A', 'x'.repeat(200))[0].alias.length, 64);
assert.deepEqual(forgetRememberedDevice(sameAddressNewTv, 'TV-A').map(row => row.hardwareId), ['TV-B']);
assert.equal(records[0].alias, undefined, 'pure operations leave their input untouched');
assert.deepEqual(renameRememberedDevice(records, 'missing', 'Alias'), records);
console.log('PASS rename and forget affect only the selected local hardware record');

const missingName = updateRememberedDevices(records, [device('TV-A', {
  name: 'Android TV', properties: { serial_number: 'TV-A', friendly_name: null },
})], later);
assert.equal(missingName[0].name, 'Living room', 'an absent friendly-name answer preserves the last known name');
assert.deepEqual(updateRememberedDevices(records, [device('TV-A')], NaN), records);

for (const raw of [null, '', '{', '[]', 'null', '{"schemaVersion":2,"devices":[]}', '{"schemaVersion":1,"devices":{}}', ' '.repeat(128 * 1024 + 1)]) {
  assert.deepEqual(deserializeRememberedDevices(raw), [], 'corrupt or unsupported storage is ignored');
}
const normalized = deserializeRememberedDevices(JSON.stringify({ schemaVersion: 1, devices: [
  ...records,
  { ...records[0], name: 'Older duplicate', lastSeen: '2025-01-01' },
  { ...records[0], name: 'Newest duplicate\u0000', lastSeen: new Date(later).toISOString(), alias: ' A\nlias ', extra: 'discarded', serial: '192.0.2.8' },
  { ...records[0], hardwareId: 'unknown' },
  { ...records[0], hardwareId: 'TV-B', lastSeen: 'not a date' },
] }));
assert.deepEqual(normalized, [{ hardwareId: 'TV-A', name: 'Newest duplicate', model: 'TV model', lastSeen: new Date(later).toISOString(), alias: 'Alias' }]);
assert.deepEqual(deserializeRememberedDevices(serializeRememberedDevices(normalized)), normalized);
console.log('PASS schema, duplicate, date, control-character and serialization normalization');

const many = updateRememberedDevices([], Array.from({ length: MAX_REMEMBERED_DEVICES + 20 }, (_, index) => device(`TV-${index}`)), firstSeen);
assert.equal(many.length, MAX_REMEMBERED_DEVICES);
const newest = updateRememberedDevices(many, [device('NEWEST')], later);
assert.equal(newest.length, MAX_REMEMBERED_DEVICES);
assert.equal(newest[0].hardwareId, 'NEWEST');
const bounded = updateRememberedDevices([], [device('TV-A', { name: 'x'.repeat(1000), model: 'y'.repeat(1000), properties: { serial_number: 'TV-A', friendly_name: 'z'.repeat(1000) } })], firstSeen);
assert.equal(bounded[0].name.length, 128);
assert.equal(bounded[0].model.length, 128);
console.log('PASS record and label bounds retain the latest known observations');

const values = new Map();
const storage = { getItem: key => values.get(key) ?? null, setItem: (key, value) => values.set(key, value) };
assert.equal(writeRememberedDevices(records, storage), true);
assert.ok(values.has(REMEMBERED_DEVICES_KEY));
assert.deepEqual(readRememberedDevices(storage), records);
const denied = { getItem() { throw new Error('blocked'); }, setItem() { throw new Error('quota'); } };
assert.deepEqual(readRememberedDevices(denied), []);
assert.equal(writeRememberedDevices(records, denied), false);
assert.deepEqual(readRememberedDevices(), [], 'server rendering needs no browser storage');
assert.equal(writeRememberedDevices(records), false);
console.log('PASS storage denial and quota failure are explicit and nonthrowing');
