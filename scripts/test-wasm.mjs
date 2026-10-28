import { readFile } from "node:fs/promises";
import process from "node:process";

const args = process.argv.slice(2);
const expectUnstamped = args[0] === "--unstamped";
const expectId = args[0] === "--expect-id" ? args[1] : undefined;
const path = args[expectUnstamped ? 1 : expectId === undefined ? 0 : 2];
if (path === undefined) {
  throw new Error(
    "usage: node scripts/test-wasm.mjs [--unstamped | --expect-id HEX] MODULE.wasm",
  );
}

const bytes = await readFile(path);
const { module, instance } = await WebAssembly.instantiate(bytes);
const pointer = instance.exports.build_id_ptr();
const length = instance.exports.build_id_len();
if (expectUnstamped) {
  if (pointer !== 0 || length !== 0) {
    throw new Error("guest returned a build ID before stamping");
  }
  console.log("unstamped guest returned no build ID");
  process.exit(0);
}
const expectedLength = expectId === undefined ? 32 : expectId.length / 2;
if (length !== expectedLength) {
  throw new Error(
    `guest returned build-ID length ${length}, expected ${expectedLength}`,
  );
}

const guestId = new Uint8Array(instance.exports.memory.buffer, pointer, length);
if (guestId.every((byte) => byte === 0)) {
  throw new Error("guest returned an unstamped build ID");
}

const sections = WebAssembly.Module.customSections(module, "build_id");
if (sections.length !== 1) {
  throw new Error(`found ${sections.length} build_id custom sections, expected 1`);
}

const payload = new Uint8Array(sections[0]);
let value = 0;
let shift = 0;
let position = 0;
for (;;) {
  const byte = payload[position++];
  value |= (byte & 0x7f) << shift;
  if ((byte & 0x80) === 0) break;
  shift += 7;
}

const customId = payload.subarray(position);
if (value !== customId.length || customId.length !== guestId.length) {
  throw new Error("malformed build_id custom section");
}
if (!customId.every((byte, index) => byte === guestId[index])) {
  throw new Error("guest and custom-section build IDs differ");
}

const guestHex = Buffer.from(guestId).toString("hex");
if (expectId !== undefined && guestHex !== expectId.toLowerCase()) {
  throw new Error(`guest returned build ID ${guestHex}, expected ${expectId}`);
}

console.log(guestHex);
