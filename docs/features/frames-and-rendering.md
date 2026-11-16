# Frames, colour conversion and rendering

> **Status:** STABLE
> **Owners (code):** `src/frame.rs`, `src/convert.rs`, `src/core.rs` (`frame_from_picture`), `src/wasm.rs` (frame and RGBA sections), `js/decoder.js` (`Frame`, `krKbFor`), `js/render.js`, `js/worker.js` (`needsRgba`)
> **Related docs:** [decoder-core.md](./decoder-core.md), [js-api-and-worker.md](./js-api-and-worker.md), [`../performance.md`](../performance.md)
> **Last verified against code:** 2026-11-15 @ `bb70073`

## Overview

A decoded picture leaves the decoder core as a `Frame`: the three planes in
one tightly packed buffer with explicit geometry and the stream's colour
metadata. From there it is either uploaded as planes to WebGL, where the
shader does YUV to RGB, or converted to RGBA in wasm (SIMD128 in the SIMD
builds) for a 2D canvas, and for every frame above 8 bits on either
renderer. The two conversions use the same Kr/Kb table so they agree on
colour. All of this is wasm-av1's; what changed is where the picture comes
from and how deep it can be (H.265 range extensions go to 16 bits).

## How it works today

### Data model

`Frame` (`src/frame.rs:162-180`): `width`, `height`, `bit_depth` (8 to 16),
`bytes_per_sample` (1, or 2 little-endian above 8 bits), `layout`
(`I400`/`I420`/`I422`/`I444`, `chroma_format_idc`'s numbering,
`src/frame.rs:28-37`), `planes: [PlaneInfo; 3]` (`offset`, `stride`,
`width`, `height`), `pts`, `color` (`matrix`, `primaries`, `transfer`,
`full_range`; 2 "unspecified" and limited range by default,
`src/frame.rs:97-117`), and `data`.

The packed layout is Y then Cb then Cr, no padding (`Frame::geometry`,
`src/frame.rs:184-213`), which is exactly what WebCodecs' `I420` expects.
rivet-h26x's `h26x::Picture` is already cropped and packed that way, so
`frame_from_picture` (`src/core.rs:282-310`) moves the picture into the
frame and copies nothing: `FrameData` keeps it (`src/frame.rs:123-135`)
and dereferences to its packed bytes, and when the frame is dropped the
buffer goes back to the decoder's pool.

The colour description is the VUI's of the SPS most recently parsed when
the unit went in (`src/core.rs:185`, `src/units.rs:273-282`, `387-393`),
carried with that unit's pts to its frame.

### Interfaces

| Kind | Name | Notes |
|---|---|---|
| wasm | `nextFrame`, `hasFrame`, `peekPts` | `src/wasm.rs:441-459` |
| wasm | `frameWidth/Height/BitDepth/BytesPerSample/Layout/Pts/Matrix/Primaries/Transfer/FullRange` | `src/wasm.rs:466-522` |
| wasm | `framePtr`, `frameLen`, `planeOffset/Stride/Width/Height(i)` | Pointers into wasm memory (`src/wasm.rs:526-558`) |
| wasm | `convertToRgba()`, `rgbaPtr`, `rgbaLen` | Into a reused scratch buffer (`src/wasm.rs:565-587`) |
| JS | `Frame` | `data`, `plane(i)`, `copyData()`, `rgba()`, `toImageData()`, `writeImageData()`, `toTransferable()` (`js/decoder.js:35-127`) |
| JS | `WebGLRenderer`, `Canvas2DRenderer`, `createRenderer(canvas, 'auto'|'webgl'|'2d')` | `js/render.js:62`, `214`, `243` |

### Flow: conversion

`Coefficients::for_matrix` (`src/convert.rs:68-79`) maps the ISO 23091-2
matrix code to Kr/Kb (BT.709, FCC, BT.601, SMPTE 240M, BT.2020); unspecified
codes use BT.709 above 576 lines and BT.601 below. Coefficients are derived
in Q16 fixed point (`src/convert.rs:47-63`). `yuv_to_rgba_with`
(`src/convert.rs:111-125`) takes the SIMD128 path for 8-bit 4:2:0 in a SIMD
build and the scalar path for every other shape and depth. Above 12 bits
the Q16 products no longer fit an `i32`, so the scalar path takes samples
to 12 bits first (`src/convert.rs:172-176`); the output is 8-bit, so
nothing visible is lost. Matrix 0 with 4:4:4 is planar GBR and is copied
across, not converted (`src/convert.rs:183`, `204-214`). Both paths use the
same integer arithmetic and are tested byte-identical
(`tests/wasm_convert.rs`, 55 combinations). `krKbFor` in JS
(`js/decoder.js:22-33`) is the same table for the shader.

### Flow: rendering

`WebGLRenderer.draw` (`js/render.js:162-193`) uploads Y/U/V as luminance
textures and converts in the fragment shader with the frame's matrix and
range; it returns `'yuv'`. For a frame of two bytes a sample (anything
above 8 bits) and for identity-matrix 4:4:4 it draws an RGBA texture from
the wasm converter instead and returns `'rgba'` (`js/render.js:165-171`).
`Canvas2DRenderer` always converts in wasm and `putImageData`s into a
reused `ImageData`. Both take a live `Frame` or the plain object a Worker
posts. Because a Worker frame has no `rgba()` to call, the decode Worker
attaches the RGBA conversion to every frame the WebGL renderer will draw
that way, even with `output: 'planes'` (`js/worker.js:69-72`). `H26xPlayer`
records the returned path in `stats.drawPath`.

### Edge cases & gotchas

- **WebGL draws frames above 8 bits through RGBA.** 10-bit and deeper video
  is converted on the CPU, in wasm, for every frame, and reaches the GPU as
  8-bit RGBA; with a Worker both the planes and the RGBA are copied out of
  wasm memory and posted. The shader path is 8-bit only. For 10-bit HEVC
  that is a cost per frame the 8-bit path does not have. It has not been
  measured: [`../performance.md`](../performance.md) times 8-bit clips,
  decode only, without drawing or RGBA.
- **Unequal luma and chroma depths are reported as the wider.**
  `bit_depth` is `max(bit_depth, bit_depth_chroma)` (`src/core.rs:302`),
  and the conversion, like the shader, reads every plane at that one depth
  (`src/convert.rs:175-180`). A stream whose chroma is deeper than its luma,
  or the reverse (allowed in H.264's High 10 and higher profiles and in
  H.265's range extensions, both of which rivet-h26x decodes at unequal
  depths), decodes correctly but converts to the wrong colours. No fixture
  has unequal depths: neither libx264 nor libx265 writes them. The planes
  themselves are right; a consumer that reads them has no field telling
  the two depths apart.
- **Views are transient.** Any call that can decode may grow wasm memory
  and detach every `Uint8Array` over it (`src/wasm.rs:13-15`). The JS
  `Frame` rebuilds its view on each access and throws once `nextFrame()` or
  `flush()` has superseded it (`js/decoder.js:66-73`).
- `rgba()` reuses one scratch buffer per decoder: the next `rgba()` call
  overwrites the previous result. Copy (`toImageData`) to keep it.
- A frame keeps rivet-h26x's picture buffer until it is dropped: frames
  held in the ring, or as `current`, are buffers the decoder's pool cannot
  reuse until then.
- Primaries and transfer are carried but not applied; HDR tone mapping is
  the renderer's job and none is implemented.

## History & references

- `1a7e686` (2026-11-15): `Frame` keeps the `h26x::Picture` (`FrameData`);
  the scalar conversion takes samples above 12 bits down to 12.
- `55e3c2b` (2026-11-15): the decode Worker sends RGBA for frames above 8
  bits in `'planes'` mode, which wasm-av1's does not (its WebGL renderer
  throws on a Worker frame above 8 bits).
- Upstream `yuv-to-rgb.c` and `draw-image.js`, both replaced in wasm-av1.
- `test/browser.mjs` draws the 10-bit H.265 and H.264 fixtures, 4:4:4 and
  monochrome; `tests/decode.rs` (`rgba_of_every_layout_and_depth`)
  converts the first frame of every fixture as a smoke test (alpha 255,
  grey stays grey, not mostly black). No fixture has unequal luma and
  chroma depths, or more than 10 bits.

## Proposed / Ideas (backlog)

| Idea | Motivation | Sketch | Status |
|---|---|---|---|
| 10-bit WebGL path | No CPU conversion and no 8-bit loss for 10-bit streams | `R16UI` or half-float textures in WebGL2, the same shader maths | Proposed |
| Carry both depths | Correct colour when luma and chroma depths differ | `bit_depth_chroma` on `Frame`, used by the scalar path and the shader | Proposed |
| WebCodecs `VideoFrame` output | Hand frames to a `<canvas>` or a track without our renderer | `new VideoFrame(frame.copyData(), {format: 'I420', ...})` | Proposed |
