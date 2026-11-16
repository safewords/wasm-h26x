# Performance

Measured on 2026-11-15 on one laptop (Apple M3 Max: 12 performance and 4
efficiency cores, macOS), in Playwright 1.62's Chromium, WebKit and
Firefox, on **CI's build of `2e79fa0`** — the `.wasm` files `pkg/` held
until `ff68fe8` (rivet-h26x `a26393e`), `wasm-opt -O3` — with
`scripts/bench-browser.mjs`. Nothing else was running. The build `pkg/`
holds now (rivet-h26x `3ec8b66`) has not been benchmarked.

**Method.** Each configuration decodes the clip inside a Worker
(`scripts/bench-worker.js`), the frames consumed there (a strided hash of
the planes, so that every configuration can be seen to produce the same
pictures — they all did), twice in one instance: the first pass starts the
thread Workers and lets the JIT tier up, the second, after a `flush()`, is
what is reported. Frames per second by the wall clock of the whole second
pass, decode only (no drawing, no RGBA). Chromium and WebKit: 300 frames,
median of three runs, each in a fresh Worker and wasm instance; WebKit was
started afresh for each clip (see "WebKit and many instances" below).
Firefox: 100 frames, one run.

**The clips** (`scripts/fetch-samples.sh`): Big Buck Bunny from
test-videos.co.uk, as published there — H.264 High, 1280×720 and 1920×1080
(the 1080p one at 60 fps), and H.265 Main, 1280×720 and 1920×1080, 30 fps,
about 4 Mb/s each, as elementary streams. Every frame of every clip decodes
to rivet-h26x's own native output (`scripts/bench.mjs` checks against
h26xdec's MD5s).

## Chromium

Frames per second (median of three; the threads builds on 2 / 4 / 8
threads):

| clip | baseline | simd | threads 2 / 4 / 8 | simd-threads 2 / 4 / 8 |
|---|---|---|---|---|
| H.264 1280×720 | 83 | 231 | 146 / 237 / 438 | 339 / 452 / 808 |
| H.264 1920×1080 | 50 | 152 | 95 / 182 / 332 | 289 / 481 / 843 |
| H.265 1280×720 | 68 | 186 | 260 / 430 / 421 | 749 / 912 / 947 |
| H.265 1920×1080 | 34 | 117 | 142 / 267 / 299 | 503 / 584 / 655 |

## WebKit

| clip | baseline | simd | threads 2 / 4 / 8 | simd-threads 2 / 4 / 8 |
|---|---|---|---|---|
| H.264 1280×720 | 91 | 221 | 149 / 203 / 347 | 326 / 374 / 533 |
| H.264 1920×1080 | 50 | 150 | 95 / 139 / 239 | 255 / 346 / 469 |
| H.265 1280×720 | 71 | 189 | 255 / 207 / 213 | 558 / 387 / 561 |
| H.265 1920×1080 | 36 | 115 | 137 / 166 / 180 | 372 / 400 / 325 |

## Firefox

Frames per second (100 frames, one run):

| clip | baseline | simd | threads 2 / 4 / 8 | simd-threads 2 / 4 / 8 |
|---|---|---|---|---|
| H.264 1280×720 | 6.2 | 21.5 | 11.9 / 23.7 / 24.3 | 28.7 / 54.1 / 64.5 |
| H.264 1920×1080 | 3.9 | 15.3 | 7.6 / 14.1 / 26.0 | 27.4 / 49.4 / 80.1 |
| H.265 1280×720 | 5.2 | 18.1 | 16.2 / 27.8 / 25.3 | 58.9 / 80.5 / 58.1 |
| H.265 1920×1080 | 2.4 | 9.8 | 10.7 / 19.2 / 17.4 | 43.0 / 62.2 / 60.8 |

## Reading it

**One thread.** The SIMD build decodes at about 2.5 to 3.4 times the
baseline's rate: rivet-h26x's `simd128` kernels carry the pixel work —
for both codecs interpolation, averaging and weighted prediction, inverse
transforms with the residual add, and the deblocking filters; for H.265
also SAO and intra prediction — and what is left is entropy decoding and
per-block bookkeeping. In Chromium and WebKit, on one
core of this machine, that is 1080p at 150 frames per second for H.264 and
115 for H.265, and 720p at about 190 to 230: comfortably real time on one
thread for 1080p at 30 or 60 fps here. A phone core is several times
slower; at the 3 to 8 times wasm-av1's measurements suggest, 720p is the
rung to start a phone on with one thread.

**Threads.** The `threads` setting is rivet-h26x's own (`H26X_THREADS`
natively), and the two codecs use it differently. H.264 runs `n` pool
workers that decode `n` pictures at once (frame threading) while the
decoding thread parses: in Chromium 2.1× / 3.2× / 5.5× the one-thread SIMD
rate at 1080p on 2 / 4 / 8 threads. H.265 runs `2n` workers for wavefront
rows, tiles, slice segments and pictures in flight, and `n/4` (at least
one) for the loop filters, because a worker waiting for the row above holds
its place: at `n = 2` that is five workers and the decoding thread, so the
H.265 rows climb faster (Chromium 720p: 4.0× at 2 threads) and level off
sooner (5.1× at 8), as the clip's pictures run out of independent rows at
these sizes. WebKit gains less from threads than Chromium and its H.265
numbers at 4 and 8 threads are noisy and lower than at 2 (WebKit reports 8
logical cores to the page; at 8 threads H.265 runs 18 Workers). The same
frames came out in every configuration.

**Firefox** (Playwright's build) runs this decoder at a tenth to a
fourteenth of Chromium's rate on one thread: 1080p at 15 fps for H.264 and
10 for H.265 in the SIMD build. wasm-av1 measured the same factor for its
decoders (rav1d and rivet-av1) in the same browser build, so it is the
engine's code for this kind of wasm, not this package; it was not
investigated further, and a release Firefox was not measured. There,
threads are what make playback possible: 1080p H.264 at 80 fps on eight
threads, 1080p H.265 at about 60 on four or eight.

## Against a native decoder, for context

FFmpeg 8.1.2's native decoders (libavcodec, AArch64 assembly), run as a
program on the same machine and the same elementary streams, `-f null`,
best of three, `-benchmark`'s wall time (`rtime`) for the first 300
frames, `-threads 1` and `-threads 8`:

| clip | FFmpeg, 1 thread | wasm SIMD, Chromium, 1 thread | FFmpeg, 8 threads | wasm SIMD + threads, Chromium, 8 threads |
|---|---|---|---|---|
| H.264 1280×720 | 404 | 231 (0.57×) | 1261 | 808 (0.64×) |
| H.264 1920×1080 | 347 | 152 (0.44×) | 1408 | 843 (0.60×) |
| H.265 1280×720 | 331 | 186 (0.56×) | 691 | 947 (1.37×) |
| H.265 1920×1080 | 209 | 117 (0.56×) | 638 | 655 (1.03×) |

So, on one thread, the SIMD build in Chromium decodes these clips at a bit
under half (H.264 1080p) to a bit over half of the rate of FFmpeg's native
decoders, with their hand-written NEON assembly, on the same core. With
eight threads it is at 0.6 of FFmpeg's rate on H.264 and level with or
ahead of it on H.265 — where the comparison is not like for like: asked
for eight threads, rivet-h26x's H.265 decoder runs eighteen workers. (rivet-h26x's own native measurements, against
libavcodec on x86-64 with every SIMD rung, are in its README.) This is
context, not a target: FFmpeg's source was not read or used for any of this.

## WebKit and many instances

Each `WorkerDecoder` (and each Worker in the benchmark) instantiates the
module with its own shared memory, declared with a 1 GiB maximum. In one
WebKit page, after about 36 such threads-build instances had been created
and their Workers terminated, `new WebAssembly.Memory` failed ("Memory" out
of memory) for every later one, and the single-threaded instances in the
same page ran at about two thirds of their usual rate. A fresh page (here,
a fresh browser per clip) was unaffected. A player should reuse one
decoder (`flush()` / `setSource()`) rather than make one per video.

## Size

See the README: 2.48 MB baseline, 2.78 MB SIMD (797 / 893 KB gzip); most
of it is the container demuxers.
