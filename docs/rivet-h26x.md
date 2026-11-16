# rivet-h26x: the decoder

`rivet-h26x` (imported as `h26x`; repository
[safewords/rivet-h26x-codecs](https://github.com/safewords/rivet-h26x-codecs),
`crates/h26x` of [rivet](https://github.com/rivet-transcoder/rivet)) is
rivet's own H.264/AVC and H.265/HEVC decoder pair: Rust, no C, no system
libraries, no build script, one dependency (`thiserror`), written from the
ITU-T Recommendations. Natively it is bit-exact against the JVT conformance
suite (all 204 AVCv1 + FRExt bitstreams and the 38 professional-profile
ones) and the JCT-VC suites (147 of 147 HEVC_v1, all 49 RExt), and it is
rivet's software decode tier. This package compiles it to WebAssembly.

Nothing of it is vendored here. `Cargo.toml` takes it from rivet `main`,
the git source `rivet-container` comes from, and `Cargo.lock` pins the
revision: rivet `f5d350e`, which carries rivet-h26x `3ec8b66`. `cargo
update -p rivet-h26x -p rivet-container` moves both.

## What this crate uses of it

| | rivet-h26x | here |
|---|---|---|
| decoding | `h264::H264Decoder`, `hevc::HevcDecoder`: `with_threads(n)`, `push_nal`, `try_next_picture`, `next_picture`, `flush` | `src/core.rs` |
| output | `Picture`: cropped planes packed Y, Cb, Cr, 8-bit as bytes and 9–16-bit as little-endian `u16`, `chroma`, both bit depths; dropped, it returns its buffer to the decoder's pool | `src/frame.rs` keeps the picture as the frame's storage — nothing is copied, and the buffer is reused |
| framing | `nal::annexb_nals`, `nal::unescape_rbsp` | `src/core.rs`, `src/units.rs`, `src/annexb.rs` |
| parameter sets | `h264::Sps`, `h264::Pps`, `hevc::Sps` (public parsers, with the VUI) | `src/units.rs`: the colour description, the frame rate, and the fields of a slice header that tell one picture from the next |
| threads | `platform::install(Platform { spawn, now_ms })` | `src/threads.rs` |
| SIMD | the `simd128` tables (`dsp::h264_wasm128`, `h264_wasm128_u16`, `hevc_wasm128`), chosen at build time by `-C target-feature=+simd128` | the `simd` and `simd-threads` variants |

What it decodes is what it decodes natively: H.264 Baseline, Main,
Extended (slice data partitioning, SP and SI slices), High, High 10, High
4:2:2, High 4:4:4 (frames, PAFF, MBAFF, CAVLC and CABAC, 8 to 14 bits,
4:0:0 to 4:4:4, equal or unequal luma and chroma depths, separate colour
planes), each picture cropped with the window of the SPS it was decoded
with; H.265 Main, Main 10, Main 12, the format range extensions (4:0:0 to
4:4:4, 8 to 16 bits, unequal depths), separate colour planes (joined into
4:4:4) and the screen content coding extensions (palette, intra block copy
— the current picture as a reference —, the adaptive colour transform,
adaptive motion vector resolution), with tiles, WPP and dependent slice
segments; and the base layer of an MV-HEVC or SHVC stream (the other layers'
NAL units are discarded, not decoded). What it still refuses —
combinations no profile allows, bit depths above 14 in H.264, SCC tools in
shapes no conformance stream has — it refuses with `Error::Unsupported`,
which `run()` reports.

The fixtures here cover 4:2:0 8- and 10-bit, 4:2:2 10-bit, 4:4:4 and 4:0:0
H.264, 4:2:0 8- and 10-bit and 4:4:4 H.265, and a two-layer H.265 stream
(`hevc-layered-main10-178x100`: `hevc-main10-178x100` with every SPS, PPS and
slice segment repeated as layer 1 by `scripts/layered-fixture.mjs`, since no
encoder run here writes a second layer), which decodes to exactly the
base stream's frames — a check of this crate's access-unit and pts
handling, since rivet-h26x `a26393e` already decoded that stream too.
Every fixture decodes to `h26xdec`'s frames in every variant on 1 to 8
threads. None exercises data partitioning, unequal depths, separate
colour planes or SCC: ffmpeg's libx264 and libx265 write none of them, nor
does rivet-h26x's own encoder; rivet-h26x tests them natively against the
conformance suites and JM/HM-generated streams.

## The threads hook: `platform`

rivet-h26x threads two ways on one FIFO pool of workers per decoder:
several pictures at once (frame threading, both codecs) and, in H.265, the
wavefront rows, tiles and slice segments of a picture. Natively the
workers are `std::thread`s. On `wasm32-unknown-unknown` std has no threads
— `std::thread::spawn` fails even in a build with atomics and shared
memory, where a thread is another instance of the module in a Web Worker
that only JavaScript can start — and no clock: `Instant::now()` panics,
and the H.265 decoder times two short waits with it.

So the threads builds need a hook, `h26x::platform::install(Platform {
spawn, now_ms })` (rivet-h26x `src/platform.rs`). Once installed,
`Pool::new` asks `spawn` for each worker instead of `std::thread`, and the
timed waits read `now_ms` through `platform::Stamp`. With nothing
installed — every native build — the pool spawns and joins std threads and
`Stamp` is `Instant`. A pool waits, as it is dropped, for the workers the
embedder started to leave their bodies (it cannot join them), so an
embedder that keeps its threads — this package keeps its Web Workers — has
them free again for the next decoder.

It was written for this package on a branch of rivet-h26x, `wasm/threads`
(`b101e69`, `a26393e`), which wasm-h26x took through a `[patch]` in
`Cargo.toml` until the branch was merged. It is now in rivet-h26x
`develop` and `main` and in rivet `main`, and the `[patch]` is gone.

`src/threads.rs` is the embedder: it hands each pool worker to a Web
Worker it keeps (`Slots`), starting a new one only when none is waiting or
starting, and reads `performance.now()` for the clock. Measured in Node:
5 Workers for a decoder of 2 threads, 9 for 4, 18 for 8 (an H.265
decoder's pool is `2n` workers for the rows and `max(n/4, 1)` for the loop
filters; an H.264 one's is `n`), and no more for any later decoder.

## H.264 cropping after an SPS change

rivet-h26x's H.264 decoded-picture buffer used to keep one cropping window,
the one of the SPS last activated, and crop each picture with it as the
picture was output. When a new SPS with another window came in — a
rendition switch to another size, or a stream that changes resolution at an
IDR picture — the pictures of the old sequence still waiting for output
were cropped with the new window: a 320x180 picture came out 320x186 in
this package's rung-switch test. `src/core.rs` worked around it by calling
`flush()` before it pushed an SPS that differed from the one of the same
id. rivet-h26x now crops each picture with the window of its own SPS, and
the workaround is gone; `a_rendition_switch_at_a_segment_boundary_is_seamless`
(`tests/container.rs`, on 1, 2 and 4 threads) fails without the workaround
on rivet-h26x `a26393e` and passes without it on `3ec8b66`.

## How fast

[performance.md](performance.md). In short: the SIMD build is about twice
the baseline's rate, and the threads builds scale with cores in Chromium
and WebKit.

## Provenance

rivet-h26x's NOTICE: the decoders are original implementations written
from the ITU-T Recommendations H.264 and H.265; they are not translations
of any other decoder's source, and the reference decoders (JM, HM) and
conformance suites were used only as programs and data to check against.
This package adds nothing to the decoding process; the work here was done
without reading any other H.264/H.265 decoder's source either — see the
README.

## Licence

Open Encoding Attribution License 1.0, source-available, not open source;
see NOTICE and LICENSE-OEAL.md.
