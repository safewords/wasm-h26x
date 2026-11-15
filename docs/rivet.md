# rivet-container: containers demuxed inside the wasm

`rivet` (https://github.com/rivet-transcoder/rivet) is a source-available
transcoding library in Rust. Its `rivet-container` crate demuxes MP4/MOV,
fragmented MP4 (moof/trun, i.e. CMAF), Matroska/WebM, MPEG-TS and AVI, and
hands out video samples as codec-native bitstreams with pts and duration.
For H.264 and H.265 that is Annex-B — the `avcC`/`hvcC` length-prefixed
NAL units converted, and the parameter sets of the sample entry put in
front of the first sample that needs them:

```rust
let d = container::streaming::demux_streaming_shared(bytes)?;   // Box<dyn StreamingDemuxer>
d.header()                       // codec ("h264", "h265"), StreamInfo, timescale
d.next_video_sample()?           // Sample { data, pts_ticks, duration_ticks }
```

That is exactly what `Decoder::push_access_unit` wants, so with the
`container` feature (default on) `Decoder::set_source_container` /
`H26xDecoder.setSourceContainer` / `Decoder.setSource` in JS take a whole
file and the demux happens in wasm. It is the same crate, used the same
way, as in [wasm-av1](https://github.com/safewords/wasm-av1) (whose
`docs/rivet.md` has the history of making it build for wasm32).

Verified here: the H.264 fixture with B-pyramids in MP4, fragmented MP4,
Matroska and TS, and the H.265 one in MP4 (`hvc1`), Matroska and TS,
decode to the elementary stream's frames, natively on 1 and 4 threads and
in every wasm variant; their pts come out in presentation order one frame
period apart.

## Segment-fed input (what HLS/CMAF needs)

rivet's demuxers are whole-buffer and forward-only, so for segmented
delivery the decoder keeps the initialisation segment and demuxes each
media segment as `init ‖ segment` (`Decoder::set_init_segment` /
`push_segment`, `setInitSegment` / `pushSegment` in wasm and JS): rivet
yields the samples with pts in the track timescale, the first with the
`avcC`/`hvcC` parameter sets in front, and they are queued as access units
— the decoder is *not* reset between segments, so frames flow continuously
across segment boundaries. A second init segment (a rendition switch) needs
nothing more for H.264, whose segments begin with an IDR picture; for H.265
an end-of-sequence NAL unit goes in front of the next segment, so that a
CRA picture there starts the decoder afresh and its RASL pictures, which
refer to the other rendition, are skipped. Tested with `testdata/cmaf/`
(init + two 1 s segments of the H.264 stream): the same frames as the
elementary stream, pts continuous; and a switch at the boundary to
`testdata/cmaf-lo/` (160x90): the first segment's frames from one, the
second's from the other, exactly.

## Cost

Most of every `.wasm` is the demuxers, as in wasm-av1: the `mp4` crate's
box readers (three monomorphisations), Matroska, and the audio-track and
subtitle extraction rivet does at construction, all reachable through
`demux_streaming_shared`, so LTO cannot drop them. The README has the
sizes with and without the feature. Builds that do not need containers
use `FEATURES= scripts/build.sh` (`FEATURES` set to nothing: no default
features), which leaves elementary streams and pushed access units.

rivet-container also depends on rivet-h26x (its SPS parsers give the VUI
colour description when an MP4 has no `colr` box). It is the same
rivet-h26x as the decoder's — one copy in the build (see
[rivet-h26x.md](rivet-h26x.md) for the `[patch]` that keeps it so while
the threads hook is on a branch).

## Licence note

rivet is under the Open Encoding Attribution License 1.0: source-available,
not open source, with a short visible credit required of commercial use
that reaches third parties. Every `.wasm` built here contains
`rivet-h26x`, and `rivet-container` too with the `container` feature;
shipping one is distribution under that licence, which asks for a copy of
the licence and its notices to go with it (see NOTICE here).
