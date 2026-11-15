// Worker side of scripts/bench-browser.mjs: decode an elementary stream as
// fast as possible with a given variant/thread count and report ms/frame. Frames are
// consumed here (a FNV hash of the planes stands in for the MD5 the native
// tests use, so configurations can be seen to agree) and never leave the
// Worker, so the number is decode alone — what the threads change.
//
// Thread spawn requests from loader.js are forwarded to the page by
// `self.postMessage` (see loader.js); the page answers them.

import { loadWasmH26x } from '../js/loader.js';
import { Decoder, Run } from '../js/decoder.js';

/** Decode up to `maxFrames` frames from the current position; wall time, worst run, hash. */
async function pass(dec, maxFrames) {
  let frames = 0;
  let hash = 0x811c9dc5;
  let worst = 0;
  const t0 = performance.now();
  while (!dec.finished && frames < maxFrames) {
    const t = performance.now();
    const r = dec.run();
    const dt = performance.now() - t;
    if (dt > worst) worst = dt;
    let f;
    while ((f = dec.nextFrame())) {
      // Hash a stride of the planes (all of it at 1080p is ~3 MB per
      // frame of JS work — enough to bias the numbers).
      const d = f.data;
      for (let i = 0; i < d.length; i += 97) hash = Math.imul(hash ^ d[i], 16777619) >>> 0;
      frames++;
      if (frames >= maxFrames) break;
    }
    if (r === Run.END_OF_STREAM) break;
    // Threads: nothing new to give, frames still being decoded.
    if (r === Run.STARVED) await new Promise((res) => setTimeout(res, 1));
  }
  return { frames, wall: performance.now() - t0, worst, hash };
}

// Two passes over the clip in one instance: the first starts the thread
// Workers and lets the JIT tier up (reported as `coldMsPerFrame`); the
// second, after a flush (the decoder made again, its threads reused), is
// the decode speed.
self.onmessage = async ({ data: { variant, baseUrl, threads, es, maxFrames } }) => {
  try {
    const rt = await loadWasmH26x({ variant, baseUrl, threads: threads > 1 });
    const dec = new Decoder(rt, { maxBuffered: 8, threads });
    dec.setSourceAnnexB(new Uint8Array(es));
    const cold = await pass(dec, maxFrames);
    dec.flush();
    const warm = await pass(dec, maxFrames);
    self.postMessage({
      type: 'done', variant: rt.variant, threads: dec.threads, frames: warm.frames,
      msPerFrame: warm.wall / warm.frames, coldMsPerFrame: cold.wall / cold.frames, worstRunMs: warm.worst,
      hash: warm.hash === cold.hash ? warm.hash : 'cold and warm passes differ', stats: dec.stats(),
    });
    dec.free?.();
  } catch (e) {
    self.postMessage({ type: 'error', message: String(e?.stack ?? e) });
  }
};
