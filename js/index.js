// @safewords/wasm-h26x — H.264 and H.265 decoding in WebAssembly, for browsers without it.
//
//   import { loadWasmH26x, Decoder } from '@safewords/wasm-h26x';
//   const rt = await loadWasmH26x();               // picks simd or baseline (threads builds: see loader.js)
//   const dec = new Decoder(rt);
//   dec.setSource(bytes);                          // .264/.265, MP4/fMP4, MKV, TS — or dec.pushAccessUnit(nals, pts)
//   dec.runUntilFull();
//   const frame = dec.nextFrame();                 // planes in wasm memory, or frame.rgba()
//
// Higher up: `H26xPlayer` (canvas + pacing, in-thread or Worker), the
// renderers, and `HlsH26xVideo` (HLS/CMAF playlists → segments → canvas,
// clocked by a `<video>` playing the audio), all documented in their own files.

export { detectSimd, detectBaseline, detectThreads, chooseVariant, VARIANTS } from './detect.js';
export { loadWasmH26x, spawnThreadWorker, THREAD_STACK_SIZE } from './loader.js';
export { Decoder, Frame, Run, Layout, Matrix, krKbFor, sniff } from './decoder.js';
export { WebGLRenderer, Canvas2DRenderer, createRenderer } from './render.js';
export { WorkerDecoder } from './worker-client.js';
export { H26xPlayer, resolveThreads } from './player.js';
export { HlsH26xVideo, BandwidthEstimator, parseMaster, parseMediaPlaylist, isH26xCodecs, h26xCodecOf } from './hls.js';
