//! Containers demuxed by rivet-container must decode to the elementary
//! stream's frames: the same streams, remuxed without re-encoding into MP4,
//! fragmented MP4, Matroska and MPEG-TS, and segmented as CMAF for HLS.
//! Their pts are the container's, in presentation order.
#![cfg(feature = "container")]

mod common;

use common::{drain, fixture, Decoded};
use wasm_h26x::{Codec, Config, Decoder, RunOutcome};

fn read(rel: &str) -> Vec<u8> {
    std::fs::read(common::testdata().join(rel)).unwrap()
}

#[test]
fn every_container_decodes_to_the_elementary_streams_frames() {
    for (stream, ext, timescale, frames_known) in [
        ("h264-cabac-b-320x180", "mp4", 12288, true),
        ("h264-cabac-b-320x180", "fmp4", 12288, false),
        ("h264-cabac-b-320x180", "mkv", 1_000_000_000, false),
        ("h264-cabac-b-320x180", "ts", 90_000, false),
        ("hevc-main-320x180", "mp4", 12288, true),
        ("hevc-main-320x180", "mkv", 1_000_000_000, false),
        ("hevc-main-320x180", "ts", 90_000, false),
    ] {
        let fx = fixture(stream);
        for threads in [1, 4] {
            let mut dec = Decoder::new(Config {
                threads,
                ..Config::default()
            })
            .unwrap();
            let info = dec
                .set_source_container(read(&format!("{stream}.{ext}")))
                .unwrap();
            let label = format!("{stream}.{ext} t{threads}");
            assert_eq!((info.width, info.height), (320, 180), "{label}");
            assert_eq!(info.timescale, timescale, "{label}: timescale");
            assert_eq!(dec.time_base(), Some((1, timescale)), "{label}");
            if frames_known {
                assert_eq!(info.total_frames, 48, "{label}: frame count");
            }
            let want = if stream.starts_with("hevc") {
                Codec::Hevc
            } else {
                Codec::H264
            };
            assert_eq!(dec.codec(), Some(want));
            let out = drain(&mut dec);
            assert_eq!(out.frames, fx.frames, "{label}: frames");
            assert_eq!(dec.stats().decode_errors, 0, "{label}");
            // Presentation order: one frame period (1/24 s) apart, to the
            // container's precision (Matroska's is a millisecond).
            let pts: Vec<i64> = out.pts.iter().map(|p| p.unwrap()).collect();
            let step = timescale as i64 / 24;
            assert!(
                pts.windows(2)
                    .all(|w| (w[1] - w[0] - step).abs() <= step / 20),
                "{label}: pts {pts:?}"
            );
        }
    }
    // Not a container, not H.264/H.265: clean errors, decoder survives.
    let mut dec = Decoder::new(Config::default()).unwrap();
    assert!(dec.set_source_container(vec![0; 100]).is_err());
    dec.set_source_annexb(fixture("h264-gray-96x64").data, None)
        .unwrap();
    assert_eq!(drain(&mut dec).frames.len(), 12);
}

/// Init + media segments pushed one at a time, as HLS feeds them.
fn segments(dir: &str, dec: &mut Decoder, out: &mut Decoded) -> usize {
    dec.set_init_segment(read(&format!("{dir}/init.mp4")))
        .unwrap();
    let mut samples = 0;
    for seg in ["seg0.m4s", "seg1.m4s"] {
        samples += dec.push_segment(&read(&format!("{dir}/{seg}"))).unwrap();
        loop {
            let r = dec.run().unwrap();
            while let Some(f) = dec.next_frame() {
                out.take(f);
            }
            if !matches!(r, RunOutcome::Consumed | RunOutcome::Full) {
                break;
            }
        }
    }
    samples
}

#[test]
fn cmaf_segments_decode_continuously() {
    // H.264 with B-pyramids; and H.265 with open GOPs, whose second segment
    // begins with a CRA picture whose RASL pictures refer to the first
    // segment: decoded continuously, they are decoded, not skipped.
    for (dir, stream, codec) in [
        ("cmaf", "h264-cabac-b-320x180", Codec::H264),
        ("cmaf-hevc", "hevc-main-320x180", Codec::Hevc),
    ] {
        let fx = fixture(stream);
        for threads in [1, 4] {
            let mut dec = Decoder::new(Config {
                max_buffered: 4,
                threads,
            })
            .unwrap();
            let mut out = Decoded::default();
            assert_eq!(segments(dir, &mut dec, &mut out), 48);
            assert_eq!(dec.codec(), Some(codec));
            assert_eq!(dec.time_base(), Some((1, 12288)));
            assert!(!dec.finished());
            dec.end_of_stream();
            let rest = drain(&mut dec);
            out.frames.extend(rest.frames);
            out.pts.extend(rest.pts);
            assert_eq!(out.frames, fx.frames, "{dir} t{threads}");
            let pts: Vec<i64> = out.pts.iter().map(|p| p.unwrap()).collect();
            if codec == Codec::H264 {
                assert!(
                    pts.windows(2).all(|w| w[1] - w[0] == 512),
                    "{dir} t{threads}: {pts:?}"
                );
            } else {
                // ffmpeg's HLS muxer writes one composition time twice at
                // the open-GOP boundary between the segments (11772 ticks,
                // as ffprobe shows too) and skips one: the frames are
                // right, and their pts are the container's, in order.
                assert!(
                    pts.windows(2).all(|w| w[1] >= w[0]),
                    "{dir} t{threads}: {pts:?}"
                );
            }
        }
    }
}

#[test]
fn a_rendition_switch_at_a_segment_boundary_is_seamless() {
    let hi = fixture("h264-cabac-b-320x180");
    let lo = fixture("h264-cmaf-lo-160x90");
    let mut dec = Decoder::new(Config {
        max_buffered: 4,
        threads: 1,
    })
    .unwrap();
    dec.set_init_segment(read("cmaf/init.mp4")).unwrap();
    assert_eq!(dec.push_segment(&read("cmaf/seg0.m4s")).unwrap(), 24);
    assert_eq!(dec.push_segment(&read("cmaf/seg1.m4s")).unwrap(), 24);
    let (first, _) = dec.last_segment_pts().unwrap();
    let mut out = Decoded::default();
    while out.frames.len() < 8 {
        dec.run().unwrap();
        while let Some(f) = dec.next_frame() {
            out.take(f);
        }
    }
    // The decoder has not reached the boundary: replace the future there.
    assert!(dec.next_queued_pts().unwrap() < first);
    assert_eq!(dec.truncate_queued_from(first), 24);
    dec.set_init_segment(read("cmaf-lo/init.mp4")).unwrap();
    assert_eq!(dec.push_segment(&read("cmaf-lo/seg1.m4s")).unwrap(), 24);
    dec.end_of_stream();
    let rest = drain(&mut dec);
    out.frames.extend(rest.frames);
    out.pts.extend(rest.pts);
    assert_eq!(out.frames.len(), 48);
    assert_eq!(out.frames[..24], hi.frames[..24], "high half exact");
    assert_eq!(out.frames[24..], lo.frames[24..], "low half exact");
    let pts: Vec<i64> = out.pts.iter().map(|p| p.unwrap()).collect();
    assert!(pts.windows(2).all(|w| w[1] - w[0] == 512), "{pts:?}");
}
