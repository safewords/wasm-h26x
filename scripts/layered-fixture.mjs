#!/usr/bin/env node
// Make a two-layer H.265 stream out of a single-layer one, for the fixture
// that checks a multi-layer (SHVC / MV-HEVC) stream decodes to its base
// layer:
//
//   node scripts/layered-fixture.mjs IN.265 OUT.265
//
// No encoder this repository runs writes more than one layer (ffmpeg's
// libx265 here is built without its alpha / multiview layers), so the second
// layer is synthetic: every SPS, PPS and slice segment of the input is
// repeated with nuh_layer_id 1 — the parameter sets right after their
// layer-0 originals, the slice segments of a picture after the last of its
// layer-0 ones, as the access unit of a two-layer stream orders them. It is
// not a conforming SHVC stream (the VPS has no extension, and a layer-1 SPS
// has its own syntax, F.7.3.2.2.1), but a single-layer decoder discards
// every NAL unit with nuh_layer_id > 0 unread (F.8.1.1), so the base layer
// must decode to exactly the input's frames, and nothing of layer 1 may
// reach the output, a frame's pts, or the parameter sets layer 0 uses.
import { readFileSync, writeFileSync } from 'node:fs';

const [input, output] = process.argv.slice(2);
if (!input || !output) {
  console.error('usage: node scripts/layered-fixture.mjs IN.265 OUT.265');
  process.exit(2);
}
const data = readFileSync(input);

// The NAL units of an Annex-B stream, without their start codes.
function nals(buf) {
  const starts = [];
  for (let i = 0; i + 2 < buf.length; i++) {
    if (buf[i] === 0 && buf[i + 1] === 0 && buf[i + 2] === 1) {
      starts.push(i + 3);
      i += 2;
    }
  }
  return starts.map((s, k) => {
    let end = k + 1 < starts.length ? starts[k + 1] - 3 : buf.length;
    // A four-byte start code's leading zero, and trailing zeros, are not the NAL unit's.
    while (end > s && buf[end - 1] === 0) end--;
    return buf.subarray(s, end);
  });
}

const START = Buffer.from([0, 0, 0, 1]);
const out = [];
const put = (nal) => out.push(START, nal);
const layer1 = (nal) => {
  const copy = Buffer.from(nal);
  if (((copy[0] & 1) << 5) | (copy[1] >> 3)) throw new Error('the input already has another layer');
  copy[1] = (copy[1] & 7) | (1 << 3);
  return copy;
};

let pending = []; // layer-1 copies of the current picture's slice segments
for (const nal of nals(data)) {
  const type = (nal[0] >> 1) & 63;
  const vcl = type < 32;
  const firstSlice = vcl && (nal[2] & 0x80) !== 0;
  // VPS, SPS, PPS, AUD, prefix SEI or the first slice segment of a picture:
  // the access unit of the slice segments held back has ended.
  if (pending.length && (firstSlice || [32, 33, 34, 35, 39].includes(type))) {
    pending.forEach(put);
    pending = [];
  }
  put(nal);
  if (type === 33 || type === 34) put(layer1(nal));
  else if (vcl) pending.push(layer1(nal));
}
pending.forEach(put);
writeFileSync(output, Buffer.concat(out));
