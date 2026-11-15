# wasm-h26x

H.264/AVC and H.265/HEVC decoding for the browser, in Rust: [rivet-h26x]
(rivet's own H.264 and H.265 decoders, written from the ITU-T
Recommendations) compiled to WebAssembly, in **baseline**, **SIMD128**,
**threads** and **SIMD + threads** builds (the decoder's worker threads as
Web Workers on shared memory), behind the same small streaming API as
[wasm-av1] — the one [GoogleChromeLabs/wasm-av1] put in front of libaom in
2018, ported to Rust. Containers (MP4, fragmented MP4 / CMAF, Matroska,
MPEG-TS) are demuxed inside the same wasm by [rivet]'s `rivet-container`
crate; Annex-B elementary streams (`.264`, `.265`) are the raw input. A
decoder only: there is no encoder in it.

Why: a site that serves H.264 or H.265 wants it to play where the browser
cannot decode it — HEVC in Firefox and in browsers without hardware HEVC,
H.264 in builds without a licensed decoder, and any profile the platform
decoder refuses (High 10, 4:2:2, 4:4:4 H.264; Main 12 and the range
extensions of H.265). This gives those pages a WebAssembly software decoder
that the browser JIT compiles to NEON/SSE where the SIMD build is used, and
that spreads over the cores where the page allows threads.

**How fast it is, plainly.** Measured on 2026-11-15 on an Apple M3 Max
laptop in Playwright's Chromium, on CI's build: on one thread the SIMD
build decodes 1080p H.264 at 152 frames per second and 1080p H.265 at 117
(720p at 231 and 186) — real time with room to spare on a laptop core, and
about half the rate of FFmpeg's native decoders with their hand-written
assembly on the same core. The baseline build is about a third of that
(50 and 34 fps at 1080p). With threads, 1080p reaches 843 fps (H.264) and
655 (H.265) on eight. WebKit is close to Chromium on one thread and gains
less from threads; Firefox (Playwright's build) runs it about ten times
slower, where threads are what make 1080p playable. The output is
bit-exact: every frame equals rivet-h26x's native decoder's.
[Performance](#performance) and [docs/performance.md](docs/performance.md)
have the tables.

**Nothing upstream is vendored.** rivet-h26x and rivet-container are git
dependencies, two packages of rivet `main` (`Cargo.lock` pins the
revision). For now rivet-h26x is patched to a branch of its own repository
that adds the hook the threads builds need
([docs/rivet-h26x.md](docs/rivet-h26x.md)); [docs/rivet.md](docs/rivet.md)
is the demuxer. `pkg/` — the built `.wasm` + glue — is committed so a git
dependency (`github:safewords/wasm-h26x#sha`) carries it;
`scripts/build.sh` regenerates it.

**Feature docs:** [`docs/features/README.md`](docs/features/README.md) — each
component as the code has it, with `path:line` references, and the known
defects.

**Changelog:** [`CHANGELOG.md`](CHANGELOG.md).

## What you get

```
pkg/baseline/wasm_h26x_bg.wasm      post-MVP wasm                     2.40 MB (766 KB gzip)
pkg/simd/wasm_h26x_bg.wasm          + simd128 kernels                 2.69 MB (858 KB gzip)
pkg/threads/wasm_h26x_bg.wasm       baseline + atomics/shared memory  2.41 MB (775 KB gzip)
pkg/simd-threads/wasm_h26x_bg.wasm  simd + atomics/shared memory      2.71 MB (872 KB gzip)
pkg/thread-worker.js               the Worker each decoder thread runs in
js/                                ESM wrapper: loader + feature detection, Decoder, WebGL/2D renderers, Worker, H26xPlayer, HlsH26xVideo
```

Those sizes are CI's build of `2e79fa0` with `wasm-opt -O3` (2026-11-15),
which is what `pkg/` holds (`pkg/manifest.json` has the bytes and the
rivet-h26x and rivet revisions; gzip is `gzip -9`). As in wasm-av1, most of
each file is the container demuxers: built without the `container` feature
(and, locally, without `wasm-opt`) the baseline module is 1.51 MB (422 KB
gzip) against 2.56 MB with it, the SIMD one 1.76 MB (505 KB gzip) against
2.82 MB. rivet-h26x's encoder is not linked: nothing here calls it.

| build | needs | Chrome | Firefox | Safari |
|---|---|---|---|---|
| baseline | post-MVP wasm (bulk memory, sign-ext, non-trapping float→int, reference types, multi-value) | 96 | 89 | 15 |
| simd | + SIMD128 | 96 | 89 | 16.4 |
| threads | baseline + shared memory and atomics, on a cross-origin-isolated page, the decoder in a Worker | 96 | 89 | 15.2 |
| simd-threads | both | 96 | 89 | 16.4 |

(The versions are those of wasm-av1's builds, which use the same features:
the highest version among the features each build needs.)

`loadWasmH26x()` picks the best one the engine validates (a one-instruction
probe per feature, as wasm-feature-detect does); `chooseVariant()` says
which without loading. Tested on 2026-11-15 in Playwright 1.62's Chromium,
Firefox and WebKit, every variant: see [Testing](#testing--what-runs-and-what-it-proves).

The threads builds give the decoder a pool of Web Workers on the same shared
memory: rivet-h26x decodes several pictures at once (frame threading, both
codecs) and the wavefront rows, tiles and slice segments of an H.265 picture
in parallel. A decoder of `n` threads uses `n` Workers for H.264 and
`2n + max(n/4, 1)` for H.265 (its pool runs more workers than cores,
because a worker waiting for a neighbouring row holds its place); the
Workers are kept and reused by every later decoder. With more than one
thread a frame therefore comes out a few access units after its data went
in, or when the input ends (`unitsInFlight` says how many may still come);
with one thread a picture comes out once the next has begun and the
reorder buffer lets it (B-pictures). They need `SharedArrayBuffer`, i.e. a
**cross-origin-isolated page** (`Cross-Origin-Opener-Policy: same-origin` +
`Cross-Origin-Embedder-Policy: require-corp` on the document), and the
decoder in a Worker (it blocks its calling thread while the others work).
`loadWasmH26x({ threads: true })` picks one when `detectThreads()` says the
page can; `H26xPlayer`/`HlsH26xVideo` take `{ worker: true, threads: 'auto' }`
and report `stats.threads`.

```js
import { loadWasmH26x, Decoder } from '@safewords/wasm-h26x';

const rt  = await loadWasmH26x();          // picks simd or baseline for this browser ({ threads: true } for a threads build, from a Worker)
const dec = new Decoder(rt);
dec.setSource(bytes);                      // .264/.265 (Annex-B), MP4/fMP4, MKV, TS — or setInitSegment()+pushSegment() for CMAF, or pushAccessUnit(nals, pts)
dec.runUntilFull();                        // decode ahead (ring of 10 frames)
const frame = dec.nextFrame();             // planes in wasm memory: frame.plane(0..2), frame.planes[i].{offset,stride,width,height}
frame.rgba();                              // or RGBA8 (SIMD128 in the SIMD build)
```

Frames are what wasm-av1 gives: the planes packed Y, Cb, Cr with no
padding, one byte per sample for 8-bit streams and two (little-endian, the
value in the low bits) above — `yuv420p` / `yuv420p10le` in ffmpeg's
vocabulary — in 4:0:0, 4:2:0, 4:2:2 or 4:4:4 as the stream has it, with the
bit depth, the colour description (matrix, primaries, transfer, range) from
the VUI, and the pts of the access unit the picture came in, in
presentation order. Nothing is converted on the way out: the frame is the
decoder's own picture (no copy), and its buffer goes back to the decoder
when the next frame replaces it. What is converted, and where:

- `frame.rgba()` / the 2D renderer: YUV→RGBA8 in wasm, every layout and
  depth (BT.601 / 709 / 2020 / 240M / FCC by the stream's matrix, limited or
  full range; planar GBR for matrix 0 at 4:4:4). 8-bit 4:2:0 takes the
  SIMD128 path in the SIMD build, byte-identical to the scalar one. Samples
  above 12 bits are taken to 12 bits first.
- The WebGL renderer: 8-bit frames as luminance textures with the matrix in
  the shader, no CPU conversion; frames above 8 bits (and planar GBR) from
  the RGBA conversion, which the decode Worker sends with each such frame.
- A stream whose luma and chroma depths differ (an H.265 range-extensions
  corner) gives two bytes for every plane and reports the wider depth.

Higher up, `H26xPlayer` (canvas, pacing by pts or by an external clock,
decode budget per animation frame, in-thread or in a Worker, stutter stats:
late frames and stalls), the renderers, and `HlsH26xVideo`: HLS master/media
playlists → CMAF init + segments (demuxed by rivet inside the wasm, no
decoder reset between segments) → canvas, clocked by a `<video>` that plays
the audio rendition, with a bandwidth estimate from its own downloads and
rung switches that take effect at the next segment boundary without a gap.
That last one is the shape a site with an existing HLS player needs — see
[docs/integration.md](docs/integration.md) for the decision, the pipeline,
rung selection, threads/isolation headers and serving. The demo page
(`npm run serve`, then http://localhost:8080/demo/) exercises the file paths.

## Consuming it

```bash
npm install @safewords/wasm-h26x
```

The package is `js/` (the ESM API) + `pkg/` (the built `.wasm` variants and
their glue) + `docs/`. `pkg/` must be served as static files, not bundled —
copy it into your assets and pass `baseUrl` to
`loadWasmH26x`/`H26xPlayer`/`HlsH26xVideo`. For the Worker, either let your
bundler pick up `new Worker(new URL('./worker.js', import.meta.url))` or —
more predictably — serve `js/` statically too and pass
`workerUrl: '<baseUrl>js/worker.js'` (its imports are relative).

Every `v*` tag is built, tested and published by `.github/workflows/release.yml`:
to npm (with provenance), and on the GitHub Release as public, unauthenticated
artefacts for consumers without a registry —
`safewords-wasm-h26x-<ver>.tgz` (the same `npm pack`, installable by URL:
`"@safewords/wasm-h26x": "https://github.com/safewords/wasm-h26x/releases/download/v<ver>/safewords-wasm-h26x-<ver>.tgz"`),
`wasm-h26x-pkg-<ver>.zip` (`pkg/` alone) and `SHA256SUMS`.
`github:safewords/wasm-h26x#v<ver>` works too (the repo is public and `pkg/`
is committed).

## Building

```bash
rustup target add wasm32-unknown-unknown            # rust-toolchain.toml pins 1.100.0
cargo install wasm-bindgen-cli --version 0.2.130 --locked   # must match Cargo.lock
brew install binaryen    # or apt/scoop — wasm-opt, optional (~15% smaller)
scripts/build.sh                                    # → pkg/{baseline,simd,threads,simd-threads}, pkg/thread-worker.js, pkg/manifest.json
```

The threads variants also go through `tools/wasm-copy-guard` (wasm-av1's,
carried over), which the script builds with cargo: in WebKit a Worker's
`memory.copy` and `memory.fill` trapped on shared memory that another
Worker grew, and the pass routes them around that
([docs/features/variants-and-threading.md](docs/features/variants-and-threading.md)).

`FEATURES= scripts/build.sh` (`FEATURES` set to nothing) builds without the
default features, that is without the container demuxers: elementary
streams and pushed access units only, and a fraction of the size. The crate
has one Cargo feature, `container`; the decoder has none.

Commit a `pkg/` only from CI's build (it pins `wasm-opt`): `gh run download
<id> -n pkg -D pkg`; after a local build, `git checkout -- pkg`. On macOS
the script needs a GNU `sed` and `stat` and a newer shell than `/bin/bash`
([docs/features/build-and-release.md](docs/features/build-and-release.md)).

## Testing — what runs, and what it proves

The reference is rivet-h26x's own native decoder (its `h26xdec` example),
which is bit-exact against the JVT and JCT-VC conformance suites: every
fixture's `testdata/*.md5` is h26xdec's MD5 of each output frame, and every
test below holds its frames to those, one by one.

| Command | Proves |
|---|---|
| `cargo test --release` (and `--no-default-features`) | Native: access-unit cutting, Exp-Golomb, sniffing, geometry, colour conversion against a float reference (8 to 16 bits); every fixture — H.264 CAVLC, CABAC with B-pyramids, High 10, 4:2:2 10-bit, 4:4:4, 4:0:0; H.265 Main (WPP), Main 10 at 178×100, 4:4:4 — decodes to h26xdec's frames on 1, 2 and 4 threads, with pts in presentation order; push mode ≡ elementary stream; flush rewinds; MP4/fMP4/MKV/TS via rivet ≡ the elementary stream on 1 and 4 threads; CMAF segments (H.264, and H.265 with open GOPs, whose RASL pictures at the boundary must be decoded); a rung switch at a segment boundary. |
| `cargo test --release --target wasm32-unknown-unknown --test wasm_convert` (± `RUSTFLAGS=-C target-feature=+simd128`) | Inside wasm, in Node: the RGBA dispatch equals the scalar path — the SIMD build's v128 code included. |
| `node --test test/decode.test.mjs` (after `scripts/build.sh`) | The built `.wasm`, all four variants, the same frames through the wasm-bindgen surface (so rivet-h26x's simd128 kernels are bit-exact too); the two threads builds on 1, 2, 4 and 8 threads, each wasm thread a `worker_threads` Worker on the shared memory, with the Workers kept and reused; SIMD and baseline RGBA **byte-identical**; push mode; containers; CMAF segments of both codecs; a seamless rung switch through the JS `Decoder`; ms/frame. |
| `node --test test/copy-guard.test.mjs` | `tools/wasm-copy-guard`, the post-link pass the threads builds get for WebKit. |
| `node test/browser.mjs` (needs Playwright — `PLAYWRIGHT_DIR` or a local install) | Headless Chromium + Firefox + WebKit: 31 combinations of variant (all four, and `auto`) × WebGL/2D × main thread/Worker × H.264/H.265 × 8-bit/10-bit/4:4:4/4:0:0 × elementary stream/MP4/fMP4/MKV/TS/HLS-CMAF (with rung switches), each played to the end with every frame shown, correctly paced, pixels on the canvas and the thread count that ran; and a live stream on four threads, pushed access unit by access unit. On 2026-11-15: 32 of 32 in each engine. |
| `node scripts/bench-browser.mjs` | Decode speed per variant and thread count in the real engines, the same frame hash in every configuration. |
| `node scripts/bench.mjs` | Decode speed under Node, every frame checked against h26xdec's when `scripts/fetch-samples.sh` wrote the MD5s. |

`scripts/make-fixtures.sh` regenerates `testdata/` with ffmpeg's libx264
and libx265 (run as programs) and h26xdec.

## Performance

Measured on 2026-11-15 on one laptop (Apple M3 Max, 12 performance + 4
efficiency cores), CI's build (`wasm-opt -O3`), `scripts/bench-browser.mjs`:
each configuration decodes the clip in a Worker twice in one instance and
the second pass is timed (decode only, wall clock); Chromium and WebKit 300
frames, median of three; Firefox 100 frames, one run. The clips are Big
Buck Bunny from test-videos.co.uk (`scripts/fetch-samples.sh`), H.264 High
and H.265 Main, about 4 Mb/s, as elementary streams. Every configuration
produced the same frames. Frames per second, one thread / the threads
builds with SIMD on 2, 4 and 8 threads:

| clip | Chromium: baseline | simd | simd-threads 2 / 4 / 8 | WebKit: simd | simd-threads 2 / 4 / 8 | Firefox: simd | simd-threads 2 / 4 / 8 |
|---|---|---|---|---|---|---|---|
| H.264 1280×720 | 83 | 231 | 339 / 452 / 808 | 221 | 326 / 374 / 533 | 21.5 | 29 / 54 / 65 |
| H.264 1920×1080 | 50 | 152 | 289 / 481 / 843 | 150 | 255 / 346 / 469 | 15.3 | 27 / 49 / 80 |
| H.265 1280×720 | 68 | 186 | 749 / 912 / 947 | 189 | 558 / 387 / 561 | 18.1 | 59 / 81 / 58 |
| H.265 1920×1080 | 34 | 117 | 503 / 584 / 655 | 115 | 372 / 400 / 325 | 9.8 | 43 / 62 / 61 |

For context, FFmpeg 8.1.2's native decoders on the same machine and
streams (run as a program, `-threads 1`): 404 / 347 fps for the H.264
clips and 331 / 209 for the H.265 ones, so the SIMD build in Chromium is
at 0.44 to 0.57 of native on one thread.

`threads` is rivet-h26x's own setting, and the codecs use it differently:
an H.264 decoder of `n` threads decodes `n` pictures at once; an H.265 one
runs `2n` workers for rows, tiles, slice segments and pictures in flight
and `max(n/4, 1)` for the loop filters, which is why H.265 climbs so fast
from one thread to two (five workers) and levels off after. WebKit's
threads numbers for H.265 are noisy and lower at 4 and 8 than at 2.
[docs/performance.md](docs/performance.md) has every variant (the baseline
and threads builds too), the native comparison at eight threads, and what
WebKit does with many instances in one page.

## Layout

```
src/annexb.rs     Annex-B elementary stream, cut into access units (wasm-av1's src/ivf.rs)
src/units.rs      access-unit boundaries, which pictures are output, VUI colour and frame rate
src/decoder.rs    run/next_frame/finished ring around the decoder core (upstream AVX_Decoder_*), elementary stream / container / push sources
src/core.rs       the core: rivet-h26x, NAL units in, pictures out, pts in presentation order
src/frame.rs      packed planes + geometry + colour metadata (the decoder's picture, kept)
src/convert.rs    YUV→RGBA: scalar (any layout, 8 to 16 bits) + wasm simd128 (8-bit 4:2:0), bit-identical
src/wasm.rs       the wasm-bindgen surface (H26xDecoder)
src/threads.rs    the threads builds: Web Workers that run rivet-h26x's pool workers
js/               loader, detect, decoder, render, worker, player, hls, thread-worker
examples/decode.rs   native CLI — a line per frame (h26xdec's MD5), PPM, timings
tests/, test/     native and Node tests (test/threads-node.mjs: a threads build on real threads in Node); test/browser.mjs the Playwright matrix
scripts/          build.sh, make-fixtures.sh, fetch-samples.sh, bench.mjs (Node), bench-browser.mjs (in the browsers), serve.mjs
docs/             rivet-h26x.md, rivet.md, performance.md, integration.md, features/
```

## Clean room

No source of any other H.264 or H.265 decoder or encoder, or of any wasm
port of one, was opened, read or searched in making this package: not
x264, x265, OpenH264, FFmpeg/libavcodec, libde265, JM, HM, Broadway.js or
tinyh264. The test streams were made by running ffmpeg with libx264 and
libx265 as programs, and the only decoder whose output the tests compare
against is rivet-h26x's own. The code here is wasm-av1's (this
organisation's) adapted to rivet-h26x (this organisation's), and the
access-unit logic in `src/units.rs` follows the text of the H.264 and H.265
Recommendations. rivet-h26x's own provenance is in its NOTICE: written from
the Recommendations, its reference decoders used only as programs.

## Licence

This repository's own code is Apache-2.0 (as wasm-av1 and upstream). The
decoder and the demuxers it is built with, rivet-h26x and rivet-container,
are under the Open Encoding Attribution License 1.0, which is
source-available and not open source: every `.wasm` built here, `pkg/` and
the npm package included, contains them, so distributing one means carrying
that licence and its notices along, and commercial use that reaches third
parties owes a short visible credit. H.264 and H.265 are covered by
patents that neither licence grants; a product or service that decodes
them is responsible for its own patent position. See NOTICE.

[rivet-h26x]: https://github.com/safewords/rivet-h26x-codecs
[wasm-av1]: https://github.com/safewords/wasm-av1
[GoogleChromeLabs/wasm-av1]: https://github.com/GoogleChromeLabs/wasm-av1
[rivet]: https://github.com/rivet-transcoder/rivet
