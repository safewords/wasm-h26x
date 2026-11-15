// Drive the built .wasm (all four variants) from Node exactly as a page would,
// and hold it, frame by frame, to the MD5s rivet-h26x's own native decoder
// gives (testdata/*.md5, from h26xdec). Run `scripts/build.sh` first;
// `npm test` does both.
//
// What this proves that `cargo test` cannot: the wasm-bindgen surface, the
// pointer/length hand-off through wasm memory, that the SIMD build (with
// rivet-h26x's simd128 kernels) decodes to the same bytes as the scalar one
// and gives byte-identical RGBA, that the threads builds decode the same
// pictures on 1, 2, 4 and 8 threads with each thread a Worker on the shared
// memory, and how fast each variant is under V8.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const testdata = path.join(root, 'testdata');
const read = (rel) => readFileSync(path.join(testdata, rel));

// Every elementary stream, with h26xdec's `index,poc,decode_index,WxH,md5` per frame.
const fixtures = readdirSync(testdata)
  .filter((f) => f.endsWith('.md5'))
  .map((f) => {
    const name = f.replace(/\.md5$/, '');
    const frames = readFileSync(path.join(testdata, f), 'utf8').trim().split('\n').filter((l) => l.includes(',')).map((l) => {
      const [, , , size, md5] = l.split(',');
      const [w, h] = size.split('x').map(Number);
      return { w, h, md5 };
    });
    const ext = name.startsWith('hevc') ? '265' : '264';
    return { name, frames, data: read(`${name}.${ext}`) };
  })
  .sort((a, b) => a.name.localeCompare(b.name));
assert.ok(fixtures.length >= 10, 'committed fixtures present');
const fixture = (name) => fixtures.find((f) => f.name === name);
const layoutOf = (name) => (name.includes('gray') ? 0 : name.includes('444') ? 3 : name.includes('422') ? 2 : 1);
const depthOf = (name) => (name.includes('10-') ? 10 : 8);

// The same probe js/detect.js uses: a module containing one v128 op.
const nodeHasSimd = WebAssembly.validate(
  new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 123, 3, 2, 1, 0, 10, 10, 1, 8, 0, 65, 0, 253, 15, 253, 98, 11]),
);

async function load(variant) {
  const dir = path.join(root, 'pkg', variant);
  const mod = await import(pathToFileURL(path.join(dir, 'wasm_h26x.js')).href);
  const bytes = readFileSync(path.join(dir, 'wasm_h26x_bg.wasm'));
  const wasm = await mod.default({ module_or_path: bytes });
  return { mod, wasm, variant };
}

const md5 = (view) => createHash('md5').update(view).digest('hex');

/** Pop every buffered frame: per-frame md5 + pts (+ RGBA md5). */
function drain(dec, wasm, out, { rgba = false } = {}) {
  while (dec.nextFrame()) {
    // Views are rebuilt per frame: memory may have grown during run().
    const view = new Uint8Array(wasm.memory.buffer, dec.framePtr(), dec.frameLen());
    out.frames.push({ w: dec.frameWidth(), h: dec.frameHeight(), md5: md5(view) });
    out.pts.push(dec.framePts());
    if (!out.first) {
      out.first = {
        bitDepth: dec.frameBitDepth(), bps: dec.frameBytesPerSample(), layout: dec.frameLayout(),
        matrix: dec.frameMatrix(), fullRange: dec.frameFullRange(),
        planes: [0, 1, 2].map((i) => [dec.planeOffset(i), dec.planeStride(i), dec.planeWidth(i), dec.planeHeight(i)]),
      };
    }
    if (rgba) {
      const t1 = performance.now();
      const ptr = dec.convertToRgba();
      out.convertMs += performance.now() - t1;
      out.rgba.update(new Uint8Array(wasm.memory.buffer, ptr, dec.rgbaLen()));
    }
  }
}

/** Decode a whole elementary stream; returns {frames, pts, first, decodeMs, convertMs, rgbaMd5, stats}. */
function decodeAll({ mod, wasm }, data, { rgba = false, codec } = {}) {
  const dec = new mod.H26xDecoder(undefined, undefined, undefined);
  dec.setSourceAnnexB(data, codec);
  const out = { frames: [], pts: [], first: null, decodeMs: 0, convertMs: 0, rgba: createHash('md5') };
  let guard = 0;
  while (!dec.finished()) {
    const t0 = performance.now();
    dec.run();
    out.decodeMs += performance.now() - t0;
    drain(dec, wasm, out, { rgba });
    assert.ok(++guard < 100000, 'decoder never finished');
  }
  const s = dec.stats();
  out.stats = { framesOut: s.framesOut, decodeErrors: s.decodeErrors, accessUnitsIn: s.accessUnitsIn };
  out.info = { codec: dec.codec(), frameCount: dec.frameCountHint(), frameRate: dec.frameRateHint(), duration: dec.durationHint() };
  dec.free();
  out.rgbaMd5 = rgba ? out.rgba.digest('hex') : null;
  return out;
}

for (const variant of ['baseline', 'simd']) {
  test(`${variant}: every fixture matches h26xdec frame by frame`, { skip: variant === 'simd' && !nodeHasSimd && 'node lacks wasm SIMD' }, async () => {
    const m = await load(variant);
    assert.equal(m.mod.simdEnabled(), variant === 'simd', 'simdEnabled() reports the build');
    assert.equal(m.mod.threadsSupported(), false);
    assert.match(m.mod.version(), /^\d+\.\d+\.\d+/);
    for (const fx of fixtures) {
      const r = decodeAll(m, fx.data);
      assert.equal(r.frames.length, fx.frames.length, `${fx.name}: frame count`);
      r.frames.forEach((f, i) => assert.deepEqual(f, fx.frames[i], `${fx.name}: frame ${i}`));
      assert.equal(r.info.codec, fx.name.startsWith('hevc') ? 'hevc' : 'h264', `${fx.name}: codec`);
      assert.equal(r.info.frameCount, fx.frames.length, `${fx.name}: frame count hint`);
      assert.equal(r.first.layout, layoutOf(fx.name), `${fx.name}: layout`);
      assert.equal(r.first.bitDepth, depthOf(fx.name), `${fx.name}: bit depth`);
      assert.equal(r.first.bps, depthOf(fx.name) > 8 ? 2 : 1, `${fx.name}: bytes/sample`);
      // An elementary stream's frames are numbered in presentation order.
      assert.deepEqual(r.pts, fx.frames.map((_, i) => i), `${fx.name}: pts`);
      assert.equal(r.stats.framesOut, fx.frames.length);
      assert.equal(r.stats.decodeErrors, 0);
      // Plane geometry is self-consistent: Y at 0, Cb after Y, Cr after Cb.
      const [y, u, v] = r.first.planes;
      assert.equal(y[0], 0);
      assert.equal(y[1], fx.frames[0].w * r.first.bps);
      if (r.first.layout !== 0) {
        assert.equal(u[0], y[1] * y[3]);
        assert.equal(v[0], u[0] + u[1] * u[3]);
      }
    }
  });
}

// The threads builds, with real threads: test/threads-node.mjs decodes every
// fixture on N threads, each wasm thread a worker_threads Worker on the
// shared memory. One process per run, so that a decoder left waiting for a
// thread is a failed test and not a hung one.
for (const variant of ['threads', 'simd-threads']) {
  for (const threads of [1, 2, 4, 8]) {
    test(`${variant}, ${threads} thread${threads > 1 ? 's' : ''}: every fixture matches h26xdec frame by frame`, { skip: variant === 'simd-threads' && !nodeHasSimd && 'node lacks wasm SIMD' }, () => {
      const run = spawnSync(process.execPath, [path.join(root, 'test', 'threads-node.mjs'), variant, String(threads)], { encoding: 'utf8', timeout: 180000 });
      assert.equal(run.error, undefined, `did not finish: ${run.error?.message}`);
      assert.equal(run.status, 0, run.stderr);
      const r = JSON.parse(run.stdout);
      assert.equal(r.threadsSupported, true, 'threadsSupported() reports the build');
      assert.equal(r.simd, variant === 'simd-threads', 'simdEnabled() reports the build');
      assert.equal(r.applied, threads, 'the decoder runs with the threads asked for');
      assert.equal(r.spawned, r.spawnedByWasm, 'every Worker asked for went through __wasmH26xSpawnThread');
      // rivet-h26x's pools: n workers for an H.264 decoder, 2n + max(n/4, 1)
      // for an H.265 one; none with one thread. The Workers are kept and
      // reused, so the second pass asks for (almost) none.
      const largest = threads > 1 ? 2 * threads + Math.max(threads >> 2, 1) : 0;
      assert.ok(r.spawnedCold >= largest, `Workers asked for: ${r.spawnedCold} < ${largest}`);
      assert.ok(r.spawned - r.spawnedCold <= Math.max(2, threads >> 1), `warm pass asked for ${r.spawned - r.spawnedCold} more Workers`);
      for (const fx of fixtures) {
        // Before the Workers are up, and once they are waiting for work.
        for (const pass of ['cold', 'warm']) {
          assert.deepEqual(r[pass][fx.name], fx.frames.map((f) => f.md5), `${fx.name}, ${pass}`);
        }
      }
      console.log(`  ${variant} t${threads}: ${r.spawnedCold} Workers cold, ${r.spawned} after the warm pass`);
    });
  }
}

test('SIMD and baseline: same frames, byte-identical RGBA', { skip: !nodeHasSimd && 'node lacks wasm SIMD' }, async () => {
  const base = await load('baseline');
  const simd = await load('simd');
  // 8-bit 4:2:0 takes the v128 conversion in the SIMD build (178x100 has a
  // width that is not a multiple of 8: the tail path); 10-bit, 4:2:2, 4:4:4
  // and 4:0:0 take the scalar path in both builds and must still agree.
  for (const fx of fixtures) {
    const a = decodeAll(base, fx.data, { rgba: true });
    const b = decodeAll(simd, fx.data, { rgba: true });
    assert.deepEqual(a.frames, b.frames, `${fx.name}: planes`);
    assert.equal(a.rgbaMd5, b.rgbaMd5, `${fx.name}: RGBA`);
  }
});

/**
 * Cut an elementary stream into access units in JS — the simple form of the
 * rule (a parameter set, SEI or delimiter after a slice, or the first slice
 * of a picture, starts the next), enough for one-slice-per-picture streams.
 */
function accessUnits(data, hevc) {
  const starts = [];
  for (let i = 0; i + 3 < data.length; i++) {
    if (data[i] === 0 && data[i + 1] === 0 && data[i + 2] === 1) starts.push(i + 3);
  }
  const units = [];
  let cur = null;
  let seenVcl = false;
  starts.forEach((s, k) => {
    const end = k + 1 < starts.length ? starts[k + 1] - 3 : data.length;
    const t = hevc ? (data[s] >> 1) & 0x3f : data[s] & 0x1f;
    const vcl = hevc ? t < 32 : t === 1 || t === 5;
    const firstSlice = vcl && (data[s + (hevc ? 2 : 1)] & 0x80) !== 0;
    const opens = hevc ? [32, 33, 34, 35, 39].includes(t) : [6, 7, 8, 9].includes(t);
    if (!cur || (seenVcl && (opens || firstSlice))) {
      cur = { from: s - 3, to: end };
      units.push(cur);
      seenVcl = false;
    }
    cur.to = end;
    if (vcl) seenVcl = true;
  });
  return units.map((u) => data.subarray(u.from, u.to));
}

test('push mode: access units fed one at a time decode identically', async () => {
  const m = await load('baseline');
  for (const name of ['h264-cavlc-320x180', 'hevc-main-320x180']) {
    const fx = fixture(name);
    const units = accessUnits(fx.data, name.startsWith('hevc'));
    assert.equal(units.length, fx.frames.length, `${name}: units`);
    const dec = new m.mod.H26xDecoder(4, 1, name.startsWith('hevc') ? 'hevc' : 'h264');
    assert.equal(dec.codec(), name.startsWith('hevc') ? 'hevc' : 'h264');
    const out = { frames: [], pts: [] };
    // Decode order. A unit's pts is its presentation time; with B-pictures
    // the order differs, and the frames still come out in presentation order
    // with the smallest pts not given out: here units are numbered in decode
    // order, so the frames come out numbered in order.
    units.forEach((u, i) => {
      dec.pushAccessUnit(u, i * 1000);
      let r;
      do {
        r = dec.run();
        drain(dec, m.wasm, out);
      } while (r === m.mod.RunResult.Consumed || r === m.mod.RunResult.Full);
    });
    assert.equal(dec.finished(), false);
    dec.endOfStream();
    while (!dec.finished()) {
      dec.run();
      drain(dec, m.wasm, out);
    }
    assert.deepEqual(out.frames, fx.frames, name);
    assert.deepEqual(out.pts, fx.frames.map((_, i) => i * 1000), `${name}: pts`);
    assert.equal(dec.width(), 320);
    dec.free();
  }
});

test('errors: what is not a stream throws, the decoder stays usable', async () => {
  const m = await load('baseline');
  const dec = new m.mod.H26xDecoder();
  assert.throws(() => dec.setSourceAnnexB(new Uint8Array([1, 2, 3])), /Annex-B/);
  assert.throws(() => dec.setSourceAnnexB(fixture('h264-gray-96x64').data, 'vp9'), /codec/);
  assert.throws(() => dec.run(), /no source/);
  dec.setSourceAnnexB(fixture('h264-gray-96x64').data);
  const r = dec.runUntilFull();
  assert.ok(r === m.mod.RunResult.Full || r === m.mod.RunResult.EndOfStream);
  assert.ok(dec.framesBuffered() > 0);
  dec.free();
});

test('elementary streams: frame rate, count and duration from the VUI', async () => {
  const m = await load('baseline');
  for (const name of ['h264-cabac-b-320x180', 'hevc-main-320x180']) {
    const dec = new m.mod.H26xDecoder();
    dec.setSourceAnnexB(fixture(name).data);
    assert.equal(dec.frameRateHint(), 24, name);
    assert.equal(dec.frameCountHint(), 48, name);
    assert.equal(dec.durationHint(), 2, name);
    assert.equal(dec.timeBaseDen() / dec.timeBaseNum(), 24, `${name}: time base is one frame`);
    dec.free();
  }
});

test('speed: decode + convert, ms/frame per variant (informational)', { skip: !nodeHasSimd && 'node lacks wasm SIMD' }, async () => {
  const rows = [];
  for (const name of ['h264-cabac-b-320x180', 'hevc-main-320x180']) {
    const fx = fixture(name);
    for (const variant of ['baseline', 'simd']) {
      const m = await load(variant);
      decodeAll(m, fx.data, { rgba: true }); // warm up the JIT
      let best = null;
      for (let i = 0; i < 5; i++) {
        const r = decodeAll(m, fx.data, { rgba: true });
        if (!best || r.decodeMs < best.decodeMs) best = r;
      }
      rows.push({ name, variant, decode: (best.decodeMs / best.frames.length).toFixed(3), convert: (best.convertMs / best.frames.length).toFixed(4) });
    }
  }
  console.log('\n  ms/frame, 320x180 (best of 5)');
  for (const r of rows) console.log(`    ${r.name.padEnd(22)} ${r.variant.padEnd(9)} decode ${r.decode}  yuv->rgba ${r.convert}`);
});

/** Decode a whole container; per-frame md5s and pts. */
function decodeContainer({ mod, wasm }, bytes) {
  const dec = new mod.H26xDecoder();
  dec.setSourceContainer(bytes);
  const info = {
    codec: dec.codec(), width: dec.width(), num: dec.timeBaseNum(), den: dec.timeBaseDen(),
    frameCount: dec.frameCountHint(), frameRate: dec.frameRateHint(),
  };
  const out = { frames: [], pts: [] };
  while (!dec.finished()) {
    dec.run();
    drain(dec, wasm, out);
  }
  dec.free();
  return { info, ...out };
}

test('containers: MP4, fragmented MP4, Matroska and TS decode to the elementary stream (rivet-container in wasm)', async () => {
  const m = await load('baseline');
  assert.equal(m.mod.containerSupport(), true, 'default build carries the container feature');
  for (const [file, timescale, count] of [
    ['h264-cabac-b-320x180.mp4', 12288, 48], ['h264-cabac-b-320x180.fmp4', 12288, 0],
    ['h264-cabac-b-320x180.mkv', 1000, 0], ['h264-cabac-b-320x180.ts', 90000, 0],
    ['hevc-main-320x180.mp4', 12288, 48], ['hevc-main-320x180.mkv', 1000, 0], ['hevc-main-320x180.ts', 90000, 0],
  ]) {
    const name = file.replace(/\.[a-z0-9]+$/, '');
    const r = decodeContainer(m, read(file));
    assert.equal(r.info.codec, name.startsWith('hevc') ? 'hevc' : 'h264', file);
    assert.equal(r.info.width, 320, file);
    assert.equal(r.info.num, 1, file);
    // Matroska's ticks are nanoseconds; rivet reads them as such.
    assert.equal(r.info.den, timescale === 1000 ? 1e9 : timescale, `${file}: timescale`);
    if (count) assert.equal(r.info.frameCount, count, `${file}: frame count hint`);
    if (count) assert.ok(Math.abs(r.info.frameRate - 24) < 0.05, `${file}: fps ${r.info.frameRate}`);
    assert.deepEqual(r.frames, fixture(name).frames, `${file}: frames`);
    for (let i = 1; i < r.pts.length; i++) assert.ok(r.pts[i] > r.pts[i - 1], `${file}: pts in presentation order`);
  }
  // Not a container: a clean error, and the decoder survives.
  const dec = new m.mod.H26xDecoder();
  assert.throws(() => dec.setSourceContainer(new Uint8Array(100)), /container/);
  dec.setSourceAnnexB(fixture('h264-gray-96x64').data);
  assert.ok(dec.runUntilFull() !== undefined);
  dec.free();
});

test('segment-fed: CMAF init + media segments pushed one at a time (rivet demux, no reset)', async () => {
  const m = await load('simd');
  const fx = fixture('h264-cabac-b-320x180');
  const dec = new m.mod.H26xDecoder(4);
  dec.setInitSegment(read('cmaf/init.mp4'));
  assert.equal(dec.timeBaseDen(), 0, 'no time base before a segment');
  const out = { frames: [], pts: [] };
  let samples = 0;
  for (const seg of ['seg0.m4s', 'seg1.m4s']) {
    samples += dec.pushSegment(read(`cmaf/${seg}`));
    let r;
    do { r = dec.run(); drain(dec, m.wasm, out); } while (r === m.mod.RunResult.Consumed || r === m.mod.RunResult.Full);
  }
  assert.equal(dec.timeBaseNum(), 1);
  assert.equal(dec.timeBaseDen(), 12288);
  assert.equal(dec.finished(), false);
  dec.endOfStream();
  while (!dec.finished()) { dec.run(); drain(dec, m.wasm, out); }
  assert.equal(samples, 48);
  assert.deepEqual(out.frames, fx.frames);
  for (let i = 1; i < out.pts.length; i++) assert.equal(out.pts[i] - out.pts[i - 1], 512, 'pts continue across segments');
  dec.free();
});

test('segment-fed H.265 with open GOPs: the RASL pictures at the boundary are decoded, not skipped', async () => {
  const m = await load('baseline');
  const dec = new m.mod.H26xDecoder(4);
  dec.setInitSegment(read('cmaf-hevc/init.mp4'));
  const out = { frames: [], pts: [] };
  let samples = 0;
  for (const seg of ['seg0.m4s', 'seg1.m4s']) {
    samples += dec.pushSegment(read(`cmaf-hevc/${seg}`));
    let r;
    do { r = dec.run(); drain(dec, m.wasm, out); } while (r === m.mod.RunResult.Consumed || r === m.mod.RunResult.Full);
  }
  assert.equal(dec.codec(), 'hevc');
  dec.endOfStream();
  while (!dec.finished()) { dec.run(); drain(dec, m.wasm, out); }
  assert.equal(samples, 48);
  assert.deepEqual(out.frames, fixture('hevc-main-320x180').frames);
  // In order (ffmpeg's HLS muxer repeats one composition time at the boundary; see tests/container.rs).
  for (let i = 1; i < out.pts.length; i++) assert.ok(out.pts[i] >= out.pts[i - 1], 'pts in order');
  dec.free();
});

test('seamless rung switch through the JS Decoder: truncate at a boundary, new init, other rung', async () => {
  const m = await load('simd');
  const { Decoder } = await import('../js/decoder.js');
  const rt = { mod: m.mod, wasm: m.wasm, variant: 'simd', simd: true, threads: false, version: m.mod.version() };
  const hi = fixture('h264-cabac-b-320x180').frames;
  const lo = fixture('h264-cmaf-lo-160x90').frames;

  const dec = new Decoder(rt, { maxBuffered: 4 });
  dec.setInitSegment(read('cmaf/init.mp4'));
  assert.equal(dec.pushSegment(read('cmaf/seg0.m4s')), 24);
  assert.equal(dec.pushSegment(read('cmaf/seg1.m4s')), 24);
  const range = dec.lastSegmentRange();
  const got = [];
  const drainJs = () => { let f; while ((f = dec.nextFrame())) got.push({ pts: f.pts, w: f.width, h: f.height, md5: md5(f.data) }); };
  while (got.length < 8) { dec.run(); drainJs(); }
  // The decoder has not reached the boundary: switch there.
  const r = dec.switchStream({ boundaryPts: range.firstPts, init: read('cmaf-lo/init.mp4') });
  assert.equal(r.ok, true);
  assert.equal(r.dropped, 24);
  assert.equal(dec.pushSegment(read('cmaf-lo/seg1.m4s')), 24);
  dec.endOfStream();
  while (!dec.finished) { dec.run(); drainJs(); }
  assert.equal(got.length, 48);
  for (let i = 1; i < got.length; i++) assert.equal(got[i].pts - got[i - 1].pts, 512, 'pts run on');
  assert.deepEqual(got.slice(0, 24).map(({ w, h, md5 }) => ({ w, h, md5 })), hi.slice(0, 24), 'high half exact');
  assert.deepEqual(got.slice(24).map(({ w, h, md5 }) => ({ w, h, md5 })), lo.slice(24), 'low half exact');
  // Too late is reported, not done.
  const late = new Decoder(rt, { maxBuffered: 4 });
  late.setInitSegment(read('cmaf/init.mp4'));
  late.pushSegment(read('cmaf/seg0.m4s'));
  late.pushSegment(read('cmaf/seg1.m4s'));
  const boundary = late.lastSegmentRange().firstPts;
  let n = 0;
  while (n < 30) { late.run(); while (late.nextFrame()) n++; }
  const r2 = late.switchStream({ boundaryPts: boundary, init: read('cmaf-lo/init.mp4') });
  assert.equal(r2.ok, false);
  assert.ok(r2.front == null || r2.front > boundary);
  late.free();
  dec.free();
});

test('JS: sniff tells elementary streams from containers', async () => {
  const { sniff } = await import('../js/decoder.js');
  assert.equal(sniff(fixture('h264-cavlc-320x180').data), 'annexb');
  assert.equal(sniff(fixture('hevc-main-320x180').data), 'annexb');
  assert.equal(sniff(read('h264-cabac-b-320x180.mp4')), 'mp4');
  assert.equal(sniff(read('cmaf/seg0.m4s')), 'mp4');
  assert.equal(sniff(read('h264-cabac-b-320x180.mkv')), 'mkv');
  assert.equal(sniff(read('h264-cabac-b-320x180.ts')), 'ts');
});

test('JS: HLS CODECS attributes', async () => {
  const { h26xCodecOf, isH26xCodecs, parseMaster } = await import('../js/hls.js');
  assert.equal(h26xCodecOf('avc1.64001f,mp4a.40.2'), 'h264');
  assert.equal(h26xCodecOf('mp4a.40.2, hvc1.1.6.L93.B0'), 'hevc');
  assert.equal(h26xCodecOf('hev1.2.4.L120.B0'), 'hevc');
  assert.equal(isH26xCodecs('av01.0.04M.08'), false);
  const master = parseMaster(readFileSync(path.join(testdata, 'cmaf', 'master.m3u8'), 'utf8'), 'http://x/cmaf/master.m3u8');
  assert.deepEqual(master.variants.map((v) => [v.height, v.codec, v.h26x]), [[90, 'h264', true], [180, 'h264', true]]);
});

test('BandwidthEstimator: two EWMAs, the estimate is the lower (a slow-down is believed at once)', async () => {
  const { BandwidthEstimator } = await import('../js/hls.js');
  const b = new BandwidthEstimator();
  assert.equal(b.estimate, 0);
  // Ten 1 s downloads of 1 MB: 8 Mb/s.
  for (let i = 0; i < 10; i++) b.sample(1, 1_000_000);
  assert.ok(Math.abs(b.estimate - 8e6) / 8e6 < 0.01, `steady: ${b.estimate}`);
  // Then a 4 s download of 1 MB (2 Mb/s): the fast average drops well below 8, the estimate follows it.
  b.sample(4, 1_000_000);
  assert.ok(b.estimate < 5e6, `slow-down believed: ${b.estimate}`);
  // A burst (1 MB in 0.1 s = 80 Mb/s) does not swing it back up as far as the burst says.
  b.sample(0.1, 1_000_000);
  assert.ok(b.estimate < 20e6, `burst not believed: ${b.estimate}`);
  // Trivial samples are ignored.
  const c = new BandwidthEstimator({ defaultEstimate: 123 });
  c.sample(0, 100);
  c.sample(1, 0);
  assert.equal(c.estimate, 123);
});
