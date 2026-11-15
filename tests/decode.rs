//! Decode every elementary-stream fixture in `testdata/` and reproduce, frame
//! by frame, the MD5s rivet-h26x's own native decoder (`h26xdec`) gives —
//! the decoder that is bit-exact against the JVT and JCT-VC conformance
//! suites. Any mismatch is a real bug: in plane packing, bit depth, the
//! driving of the decoder core, access-unit splitting, or the core.
//!
//! `testdata/NAME.md5` is h26xdec's output (`scripts/make-fixtures.sh`).

mod common;

use common::{drain, fixture, fixtures, Decoded};
use wasm_h26x::{convert, Codec, Config, Decoder, PixelLayout, RunOutcome};

fn decode_annexb(data: &[u8], config: Config, codec: Option<Codec>) -> (Decoded, wasm_h26x::Stats) {
    let mut dec = Decoder::new(config).unwrap();
    dec.set_source_annexb(data.to_vec(), codec).unwrap();
    let out = drain(&mut dec);
    (out, dec.stats())
}

#[test]
fn every_fixture_matches_h26xdec_frame_by_frame() {
    let all = fixtures();
    assert!(
        all.len() >= 10,
        "the committed fixtures, found {}",
        all.len()
    );
    for fx in &all {
        for threads in [1, 2, 4] {
            let (out, stats) = decode_annexb(
                &fx.data,
                Config {
                    threads,
                    ..Config::default()
                },
                None,
            );
            assert_eq!(
                out.frames.len(),
                fx.frames.len(),
                "{} t{threads}: frames",
                fx.name
            );
            for (i, (got, want)) in out.frames.iter().zip(&fx.frames).enumerate() {
                assert_eq!(got, want, "{} t{threads}: frame {i}", fx.name);
            }
            assert_eq!(stats.decode_errors, 0, "{}", fx.name);
            assert_eq!(stats.frames_out as usize, fx.frames.len());
            // An elementary stream's frames are numbered in presentation order.
            let pts: Vec<i64> = out.pts.iter().map(|p| p.unwrap()).collect();
            assert_eq!(
                pts,
                (0..fx.frames.len() as i64).collect::<Vec<_>>(),
                "{}: pts",
                fx.name
            );
            let first = out.first.unwrap();
            assert_eq!(first.layout, fx.layout(), "{}: layout", fx.name);
            assert_eq!(first.bit_depth, fx.bit_depth(), "{}: depth", fx.name);
            assert_eq!(
                first.bytes_per_sample,
                if fx.bit_depth() > 8 { 2 } else { 1 }
            );
            // Planes packed as Frame::geometry says: Y at 0, Cb after Y, Cr after Cb.
            let (planes, total) = wasm_h26x::Frame::geometry(
                first.width,
                first.height,
                first.bytes_per_sample as usize,
                first.layout,
            );
            assert_eq!(first.planes, planes, "{}: plane geometry", fx.name);
            assert_eq!(first.data.len(), total);
        }
    }
}

#[test]
fn codec_is_told_from_the_stream_or_given() {
    let h264 = fixture("h264-cavlc-320x180");
    let hevc = fixture("hevc-main-320x180");
    let mut dec = Decoder::new(Config::default()).unwrap();
    dec.set_source_annexb(h264.data.clone(), None).unwrap();
    assert_eq!(dec.codec(), Some(Codec::H264));
    dec.set_source_annexb(hevc.data.clone(), None).unwrap();
    assert_eq!(dec.codec(), Some(Codec::Hevc));
    // Given explicitly, it is believed: H.265 data fed to the H.264 decoder
    // is NAL unit types H.264 ignores or refuses, and nothing comes out, but
    // nothing panics and the decoder stays usable.
    dec.set_source_annexb(hevc.data.clone(), Some(Codec::H264))
        .unwrap();
    let mut frames = 0;
    loop {
        if let Ok(RunOutcome::EndOfStream) = dec.run() {
            break;
        }
        while dec.next_frame().is_some() {
            frames += 1;
        }
    }
    assert_eq!(frames, 0);
    let (out, _) = decode_annexb(&hevc.data, Config::default(), Some(Codec::Hevc));
    assert_eq!(out.frames, hevc.frames);
    // Not an elementary stream at all.
    assert!(dec.set_source_annexb(b"DKIF".to_vec(), None).is_err());
}

#[test]
fn the_frame_rate_comes_from_the_vui() {
    let mut dec = Decoder::new(Config::default()).unwrap();
    let r = dec
        .set_source_annexb(fixture("h264-cabac-b-320x180").data, None)
        .unwrap();
    assert_eq!(r.frame_count(), 48);
    // x264 and x265 write timing information for 24 frames per second.
    assert_eq!(dec.vui_frame_rate(), Some(24.0));
    assert_eq!(dec.time_base().map(|(n, d)| d / n), Some(24));
    dec.set_source_annexb(fixture("hevc-main-320x180").data, None)
        .unwrap();
    assert_eq!(dec.vui_frame_rate(), Some(24.0));
}

#[test]
fn push_mode_matches_the_elementary_stream() {
    for name in ["h264-cabac-b-320x180", "hevc-main-320x180"] {
        let fx = fixture(name);
        let mut reader = wasm_h26x::AnnexBReader::new(fx.data.clone(), None).unwrap();
        let mut units = Vec::new();
        while let Some(u) = reader.next_unit() {
            units.push((u.data.to_vec(), u.pts * 1000));
        }
        for threads in [1, 3] {
            let mut dec = Decoder::new(Config {
                max_buffered: 4,
                threads,
            })
            .unwrap();
            let mut out = Decoded::default();
            for (data, pts) in &units {
                dec.push_access_unit(data.clone(), *pts).unwrap();
                // Keep the ring bounded, like a player between segments.
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
            assert!(!dec.finished(), "not before the end of stream");
            dec.end_of_stream();
            out.frames.extend(drain(&mut dec).frames);
            assert_eq!(out.frames, fx.frames, "{name} t{threads}");
            let pts: Vec<i64> = out.pts.iter().map(|p| p.unwrap()).collect();
            assert!(
                pts.windows(2).all(|w| w[1] == w[0] + 1000),
                "{name}: {pts:?}"
            );
        }
    }
}

#[test]
fn flush_rewinds_an_elementary_stream() {
    let fx = fixture("hevc-main10-178x100");
    let mut dec = Decoder::new(Config {
        max_buffered: 3,
        threads: 2,
    })
    .unwrap();
    dec.set_source_annexb(fx.data.clone(), None).unwrap();
    // Part of the way in, with frames in flight and in the ring.
    for _ in 0..7 {
        dec.run().unwrap();
        dec.next_frame();
    }
    dec.flush().unwrap();
    let out = drain(&mut dec);
    assert_eq!(out.frames, fx.frames);
}

#[test]
fn the_ring_holds_max_buffered_and_finished_waits_for_it() {
    let fx = fixture("h264-high10-160x90");
    let mut dec = Decoder::new(Config {
        max_buffered: 5,
        threads: 1,
    })
    .unwrap();
    dec.set_source_annexb(fx.data.clone(), None).unwrap();
    assert_eq!(dec.run_until_full().unwrap(), RunOutcome::Full);
    assert_eq!(dec.frames_buffered(), 5);
    assert!(!dec.finished());
    let first = dec.next_frame().unwrap().clone();
    assert_eq!(first.pts, Some(0));
    assert_eq!((first.width, first.height), (160, 90));
    assert_eq!(dec.width(), Some(160));
}

#[test]
fn rgba_of_every_layout_and_depth() {
    // The scalar conversion runs on each kind of frame the fixtures give,
    // and a mid-grey picture region comes out grey-ish: a smoke test that
    // the geometry the converter reads is the decoder's.
    for fx in fixtures() {
        let mut dec = Decoder::new(Config::default()).unwrap();
        dec.set_source_annexb(fx.data.clone(), None).unwrap();
        dec.run_until_full().unwrap();
        let f = dec.next_frame().unwrap();
        let mut out = vec![0u8; f.rgba_len()];
        convert::yuv_to_rgba(f, &mut out);
        assert!(out.chunks(4).all(|p| p[3] == 255), "{}", fx.name);
        if f.layout == PixelLayout::I400 {
            assert!(
                out.chunks(4).all(|p| p[0] == p[1] && p[1] == p[2]),
                "{}",
                fx.name
            );
        }
        let nonblack = out
            .chunks(4)
            .filter(|p| p[0] as u32 + p[1] as u32 + p[2] as u32 > 60)
            .count();
        assert!(nonblack * 2 > out.len() / 4, "{}: mostly black", fx.name);
    }
}
