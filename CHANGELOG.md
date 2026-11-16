# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
A release is a `v*` tag matching the version in `package.json` and
`Cargo.toml`; `release.yml` publishes it to the GitHub Release and to npm as
`@safewords/wasm-h26x`.

## [0.1.0] - Unreleased

The first version: [wasm-av1](https://github.com/safewords/wasm-av1) 0.3.2
for H.264 and H.265, with rivet-h26x in place of rivet-av1.

### Added

- **rivet-h26x compiled to WebAssembly** in the four variants wasm-av1 has —
  baseline, SIMD128 (rivet-h26x's `simd128` kernels for both codecs, 8-bit
  and deeper), threads and SIMD + threads — with the same feature detection
  and loader (`loadWasmH26x`). What rivet-h26x decodes natively: H.264
  Baseline to High 4:4:4, Extended with slice data partitioning, equal or
  unequal luma and chroma depths, each picture cropped with its own SPS's
  window; H.265 Main, Main 10, Main 12, the range extensions, separate
  colour planes and screen content coding (palette, intra block copy, the
  adaptive colour transform, adaptive MV resolution), and the base layer of
  SHVC / MV-HEVC streams. Every fixture decodes to rivet-h26x's own native
  output frame by frame in every variant. rivet-h26x and rivet-container
  come from rivet `main` (`f5d350e`, rivet-h26x `3ec8b66`); rivet-h26x's
  encoders are not linked.
- **Input:** Annex-B elementary streams (`setSourceAnnexB`, codec told from
  the NAL units or given), cut into access units here; MP4, fragmented MP4 /
  CMAF, Matroska and MPEG-TS demuxed in the wasm by rivet-container
  (`setSourceContainer`; `setSource` picks); CMAF segments
  (`setInitSegment` / `pushSegment`) with seamless rendition switches; and
  access units pushed one at a time (`pushAccessUnit`; `pushTemporalUnit`
  is kept as an alias). Each frame carries the pts of the access unit its
  picture came in, in presentation order, and the colour description and
  frame rate of the stream's VUI.
- **Frames** in wasm-av1's layout — packed Y, Cb, Cr, one byte per sample
  at 8 bits and little-endian 16-bit above, 4:0:0 to 4:4:4 — taken from the
  decoder without a copy. The RGBA conversion (scalar, and `simd128` for
  8-bit 4:2:0) takes 8 to 16 bits; the WebGL renderer draws 8-bit frames in
  its shader and deeper ones from that conversion, which the decode Worker
  now sends with every such frame.
- **Threads:** rivet-h26x's frame threading, and H.265's wavefront rows,
  tiles and slice segments, on Web Workers on shared memory — rivet-h26x's
  pool started through its `platform` hook, the Workers kept and reused
  across decoders.
- The JS API of wasm-av1 under new names: `Decoder`, `WorkerDecoder`,
  `H26xPlayer`, `HlsH26xVideo` (CODECS `avc1`/`avc3`/`hvc1`/`hev1`), the
  renderers, `sniff` (`'annexb' | 'mp4' | 'mkv' | 'ts'`).
- Tests: native (every fixture frame by frame on 1, 2 and 4 threads, every
  container, CMAF segments, a rendition switch on 1, 2 and 4 threads, a
  two-layer H.265 stream), Node (every variant, the threads builds on 1 to
  8 threads), and Playwright (33 combinations in each of Chromium, Firefox
  and WebKit). CI and release workflows as wasm-av1's.

### Known

- Unequal luma and chroma bit depths (H.264 High profiles, H.265 range
  extensions) decode exactly, but the frame reports the wider depth and
  the RGBA conversion and the WebGL shader read every plane at it, so such
  a stream shows the wrong colours (docs/features/frames-and-rendering.md).
- In one WebKit page, a few dozen threads-build instances (each with its
  own shared memory) exhaust it; reuse one decoder (docs/performance.md).
- Pictures that are decoded but not output (`pic_output_flag` 0, a refused
  picture) would leave their pts behind and shift later frames' pts by one
  (docs/features/decoder-core.md).
