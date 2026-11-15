//! Annex-B elementary streams (`.264`, `.265`, `.h264`, `.hevc`): the raw
//! input, as IVF is for wasm-av1.
//!
//! An elementary stream is NAL units behind start codes and nothing else —
//! no sample boundaries, no timestamps. [`AnnexBReader`] takes a whole one,
//! cuts it into access units once, up front, with the [`Tracker`] (H.264
//! 7.4.1.2.3, H.265 7.4.2.4.4), and hands them out one at a time, each with
//! a pts: how many frames' pictures came before it in decode order. The
//! decoder gives frames back in presentation order and each takes the
//! smallest pts not given out yet ([`crate::core`]), so the frames come out
//! numbered 0, 1, 2, … in the order they are shown. With the frame rate the
//! VUI gives ([`AnnexBReader::frame_duration`]) that number is a time;
//! without it the player falls back to a rate of its own.
//! Cutting up front is cheap (a scan for start codes, and the first bytes of
//! each slice header) and gives the frame count and a free rewind.

use std::fmt;

use crate::units::{Codec, Tracker};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnexBError {
    /// No start code: not an elementary stream.
    NoStartCode,
    /// Start codes, but neither H.264's nor H.265's NAL units behind them.
    UnknownCodec,
}

impl fmt::Display for AnnexBError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnnexBError::NoStartCode => write!(f, "not an Annex-B stream (no start code)"),
            AnnexBError::UnknownCodec => write!(
                f,
                "Annex-B stream whose NAL units are neither H.264 nor H.265 (say which with the codec argument)"
            ),
        }
    }
}

impl std::error::Error for AnnexBError {}

/// One access unit: a byte range of the stream, and its pts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Unit {
    start: usize,
    end: usize,
    pts: i64,
}

/// An access unit as [`AnnexBReader::next_unit`] hands it out.
#[derive(Debug, PartialEq, Eq)]
pub struct AccessUnit<'a> {
    /// The NAL units, with their start codes.
    pub data: &'a [u8],
    /// Frames begun before it in decode order. (A unit that begins no frame
    /// of its own — a second field — carries one too; it goes unused.)
    pub pts: i64,
}

/// A whole elementary stream, cut into access units.
pub struct AnnexBReader {
    data: Vec<u8>,
    codec: Codec,
    units: Vec<Unit>,
    pos: usize,
    frames: u64,
    frame_duration: Option<(u32, u32)>,
}

impl AnnexBReader {
    /// Take a whole stream. `codec` says which it is; `None` guesses from
    /// the first NAL units ([`crate::units::sniff`]).
    pub fn new(data: Vec<u8>, codec: Option<Codec>) -> Result<AnnexBReader, AnnexBError> {
        if !starts_with_start_code(&data) {
            return Err(AnnexBError::NoStartCode);
        }
        let codec = codec
            .or_else(|| crate::units::sniff(&data))
            .ok_or(AnnexBError::UnknownCodec)?;
        let mut tracker = Tracker::new(codec);
        let mut units = Vec::new();
        let mut current: Option<Unit> = None;
        let mut frames: i64 = 0;
        let base = data.as_ptr() as usize;
        for nal in h26x::nal::annexb_nals(&data) {
            let offset = nal.as_ptr() as usize - base;
            // Back over the start code: three bytes, and a fourth zero if
            // there is one (a zero before a start code is the previous
            // unit's trailing zero either way, harmless on both sides).
            let start = offset.saturating_sub(3);
            let ev = tracker.nal(nal);
            if ev.starts_unit {
                if let Some(mut u) = current.take() {
                    u.end = start;
                    units.push(u);
                }
            }
            let unit = current.get_or_insert(Unit {
                start,
                end: data.len(),
                pts: frames,
            });
            if ev.new_frame {
                // A unit has one primary picture; should it have more, the
                // pts are the first's, and the count still right.
                if unit.pts > frames {
                    unit.pts = frames;
                }
                frames += 1;
            }
        }
        if let Some(mut u) = current.take() {
            u.end = data.len();
            units.push(u);
        }
        Ok(AnnexBReader {
            frame_duration: tracker.frame_duration(),
            data,
            codec,
            units,
            pos: 0,
            frames: frames as u64,
        })
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    /// Access units in the stream.
    pub fn unit_count(&self) -> usize {
        self.units.len()
    }

    /// Frames the stream will show (pictures that are output, a field pair
    /// counted once).
    pub fn frame_count(&self) -> u64 {
        self.frames
    }

    /// Seconds per frame as `(num, den)`, from the first sequence parameter
    /// set's VUI timing information, if it has one.
    pub fn frame_duration(&self) -> Option<(u32, u32)> {
        self.frame_duration
    }

    /// The next access unit, or `None` at the end.
    pub fn next_unit(&mut self) -> Option<AccessUnit<'_>> {
        let u = *self.units.get(self.pos)?;
        self.pos += 1;
        Some(AccessUnit {
            data: &self.data[u.start..u.end],
            pts: u.pts,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.units.len()
    }

    /// Back to the first access unit.
    pub fn rewind(&mut self) {
        self.pos = 0;
    }
}

/// A byte stream begins with a start code, after any number of zero bytes
/// (B.2: `leading_zero_8bits`, then `zero_byte` and `start_code_prefix_one_3bytes`).
pub fn starts_with_start_code(data: &[u8]) -> bool {
    let zeros = data.iter().take_while(|&&b| b == 0).count();
    zeros >= 2 && data.get(zeros) == Some(&1)
}

impl fmt::Debug for AnnexBReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnnexBReader")
            .field("codec", &self.codec)
            .field("bytes", &self.data.len())
            .field("units", &self.units.len())
            .field("pos", &self.pos)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_what_is_not_annex_b() {
        assert_eq!(
            AnnexBReader::new(b"DKIF....".to_vec(), None).unwrap_err(),
            AnnexBError::NoStartCode
        );
        // A start code in front of something neither codec starts with.
        assert_eq!(
            AnnexBReader::new(vec![0, 0, 1, 0x7c, 0xff], None).unwrap_err(),
            AnnexBError::UnknownCodec
        );
    }

    #[test]
    fn units_cover_the_stream_in_order() {
        let data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/h264-cabac-b-320x180.264"
        ))
        .unwrap();
        let len = data.len();
        let mut r = AnnexBReader::new(data, None).unwrap();
        assert_eq!(r.codec(), Codec::H264);
        assert_eq!(r.unit_count(), 48);
        assert_eq!(r.frame_count(), 48);
        // x264 writes timing information: 24 frames per second.
        assert_eq!(r.frame_duration().map(|(n, d)| d / n), Some(24));
        let mut end = 0;
        let mut pts = Vec::new();
        while let Some(u) = r.next_unit() {
            assert!(u.data.starts_with(&[0, 0, 1]) || u.data.starts_with(&[0, 0, 0, 1]));
            end += u.data.len();
            pts.push(u.pts);
        }
        // Every byte of the stream is in exactly one unit (the first starts
        // after a leading zero of its four-byte start code).
        assert!(len - end <= 1, "{len} {end}");
        // Each unit's pts is its picture's place in decode order.
        assert_eq!(pts, (0..48).collect::<Vec<_>>());
        assert!(r.is_empty());
        r.rewind();
        assert_eq!(r.next_unit().unwrap().pts, 0);
    }
}
