//! Access units: where one ends, and which pictures in it will be shown.
//!
//! rivet-h26x takes NAL units one at a time and finds picture boundaries
//! itself; it does not need access units. This crate does, twice:
//!
//! * an elementary stream ([`crate::annexb`]) is cut into access units so
//!   that [`crate::Decoder::run`] does a bounded amount of work per call —
//!   one picture — as it does for every other source;
//! * a timestamp comes in with each access unit (a container sample's pts),
//!   and has to go out on the right frame. Frames leave the decoder in
//!   output order, and in a conforming stream output order is presentation
//!   order, so the earliest pts not yet given out belongs to the next frame
//!   ([`crate::core`]). That holds only if every pts that goes in has a
//!   frame coming out, so the units whose pictures are *not* output — the
//!   second field of an H.264 field pair (one frame from two pictures) and
//!   the RASL pictures an H.265 decoder skips after the random access point
//!   it starts at — must be recognised, and their pts left out.
//!
//! The [`Tracker`] does both from the NAL unit headers and the first few
//! fields of each slice header, following clause 7.4.1.2 of H.264 (order of
//! NAL units and access units, and the detection of the first VCL NAL unit of
//! a primary coded picture) and clause 7.4.2.4 of H.265. The parameter sets
//! are parsed with rivet-h26x's own public parsers, which also give the
//! colour description and frame rate in their VUI.

use std::collections::HashMap;

use h26x::nal::unescape_rbsp;

use crate::frame::ColorInfo;

/// The two codecs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    /// H.264 / AVC.
    H264,
    /// H.265 / HEVC.
    Hevc,
}

impl Codec {
    /// From a container's or a caller's label: `h264`/`avc`/`avc1`/`avc3`,
    /// `h265`/`hevc`/`hvc1`/`hev1` (any case).
    pub fn from_label(label: &str) -> Option<Codec> {
        match label.to_ascii_lowercase().as_str() {
            "h264" | "avc" | "avc1" | "avc3" | "264" => Some(Codec::H264),
            "h265" | "hevc" | "hvc1" | "hev1" | "265" => Some(Codec::Hevc),
            _ => None,
        }
    }

    /// `"h264"` or `"hevc"`.
    pub fn name(self) -> &'static str {
        match self {
            Codec::H264 => "h264",
            Codec::Hevc => "hevc",
        }
    }
}

/// Which codec Annex-B data holds, judged from the headers of its first NAL
/// units; `None` when they do not say (no start code, or nothing either
/// codec would begin a stream with).
///
/// The two header layouts overlap, but the units that open a stream do not:
/// an H.264 SPS (`0x67`) read as H.265 has a nonzero layer id, and an H.265
/// VPS, SPS or PPS (`0x40`, `0x42`, `0x44`) read as H.264 is a type H.264
/// does not start with.
pub fn sniff(data: &[u8]) -> Option<Codec> {
    let (mut h264, mut hevc) = (0u32, 0u32);
    for nal in h26x::nal::annexb_nals(data).take(32) {
        let Some(&b0) = nal.first() else { continue };
        if b0 & 0x80 != 0 {
            continue;
        }
        match b0 & 0x1f {
            7 | 8 | 5 => h264 += 2,
            1 | 9 => h264 += 1,
            _ => {}
        }
        if let Some(&b1) = nal.get(1) {
            let layer = ((b0 & 1) << 5) | (b1 >> 3);
            if layer == 0 && b1 & 7 != 0 {
                match (b0 >> 1) & 0x3f {
                    32..=35 | 19..=21 => hevc += 2,
                    0..=9 | 16..=18 | 39 | 40 => hevc += 1,
                    _ => {}
                }
            }
        }
    }
    match h264.cmp(&hevc) {
        std::cmp::Ordering::Greater => Some(Codec::H264),
        std::cmp::Ordering::Less => Some(Codec::Hevc),
        std::cmp::Ordering::Equal => None,
    }
}

/// What one NAL unit means for the access units around it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NalEvent {
    /// It is the first NAL unit of a new access unit (only ever true once a
    /// VCL NAL unit has been seen in the current one).
    pub starts_unit: bool,
    /// It begins a picture that will be output as a frame of its own: the
    /// first slice of a new primary picture that is neither the second
    /// field of a pair nor a RASL picture the decoder skips.
    pub new_frame: bool,
    /// It is an H.264 sequence parameter set that replaces a different one
    /// of the same id: a new coded video sequence, perhaps of another size
    /// (see `Core::send`).
    pub sps_changed: bool,
}

/// The fields of an H.264 slice header that tell one primary coded picture
/// from the next (7.4.1.2.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SliceKey {
    frame_num: u32,
    pps_id: u32,
    field_pic: bool,
    bottom_field: bool,
    non_ref: bool,
    idr: bool,
    idr_pic_id: u32,
    poc_lsb: u32,
    delta_poc_bottom: i32,
    delta_poc: [i32; 2],
}

/// The last H.264 picture begun, for pairing fields.
#[derive(Clone, Copy, Debug)]
struct LastPicture {
    field: bool,
    bottom: bool,
    frame_num: u32,
    non_ref: bool,
    /// A second field has joined it already.
    paired: bool,
}

/// Follows a stream NAL unit by NAL unit. See the [module docs](self).
#[derive(Debug)]
pub struct Tracker {
    codec: Codec,
    sps264: HashMap<u32, h26x::h264::Sps>,
    pps264: HashMap<u32, h26x::h264::Pps>,
    last_slice: Option<SliceKey>,
    last_picture: Option<LastPicture>,
    /// H.265: the RASL pictures of the current IRAP are skipped (the
    /// decoder's NoRaslOutputFlag; set until an IRAP says otherwise).
    skip_rasl: bool,
    /// H.265: no picture has been decoded since the stream started (or since
    /// an end of sequence), so the next IRAP's RASL pictures are skipped.
    stream_start: bool,
    /// A VCL NAL unit has been seen in the current access unit.
    seen_vcl: bool,
    color: ColorInfo,
    /// Seconds per frame as `(num, den)`, from the VUI timing information.
    frame_duration: Option<(u32, u32)>,
}

impl Tracker {
    pub fn new(codec: Codec) -> Tracker {
        Tracker {
            codec,
            sps264: HashMap::new(),
            pps264: HashMap::new(),
            last_slice: None,
            last_picture: None,
            skip_rasl: true,
            stream_start: true,
            seen_vcl: false,
            color: ColorInfo::default(),
            frame_duration: None,
        }
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    /// The stream starts over (a seek, or a decoder reset): what the next
    /// picture is cannot depend on the pictures before. Parameter sets are
    /// kept — they stay valid until replaced.
    pub fn restart(&mut self) {
        self.last_slice = None;
        self.last_picture = None;
        self.skip_rasl = true;
        self.stream_start = true;
        self.seen_vcl = false;
    }

    /// The colour description of the most recent sequence parameter set.
    pub fn color(&self) -> ColorInfo {
        self.color
    }

    /// Seconds per frame as `(num, den)`, if the VUI says.
    pub fn frame_duration(&self) -> Option<(u32, u32)> {
        self.frame_duration
    }

    /// Take one NAL unit (without its start code).
    pub fn nal(&mut self, nal: &[u8]) -> NalEvent {
        match self.codec {
            Codec::H264 => self.nal_h264(nal),
            Codec::Hevc => self.nal_hevc(nal),
        }
    }

    /// A NAL unit type that, after a VCL NAL unit, begins the next access
    /// unit: `seen_vcl` is cleared and the event says so.
    fn opens_unit(&mut self) -> NalEvent {
        let starts_unit = std::mem::take(&mut self.seen_vcl);
        NalEvent {
            starts_unit,
            ..NalEvent::default()
        }
    }

    fn nal_h264(&mut self, nal: &[u8]) -> NalEvent {
        let Some(&b0) = nal.first() else {
            return NalEvent::default();
        };
        match b0 & 0x1f {
            1 | 5 => {
                let key = self.h264_slice_key(nal);
                let new_picture = match (key, self.last_slice) {
                    (Some(k), Some(last)) => k != last,
                    (Some(_), None) => true,
                    // A slice whose parameter sets are unknown (the decoder
                    // will refuse it): a picture of its own only if nothing
                    // of this access unit was a slice yet.
                    (None, _) => !self.seen_vcl,
                };
                let starts_unit = self.seen_vcl && new_picture;
                let mut new_frame = false;
                if new_picture {
                    let second_field = key.is_some_and(|k| {
                        k.field_pic
                            && !k.idr
                            && self.last_picture.is_some_and(|p| {
                                p.field
                                    && !p.paired
                                    && p.bottom != k.bottom_field
                                    && p.frame_num == k.frame_num
                                    && p.non_ref == k.non_ref
                            })
                    });
                    if second_field {
                        if let Some(p) = self.last_picture.as_mut() {
                            p.paired = true;
                        }
                    } else {
                        self.last_picture = key.map(|k| LastPicture {
                            field: k.field_pic,
                            bottom: k.bottom_field,
                            frame_num: k.frame_num,
                            non_ref: k.non_ref,
                            paired: false,
                        });
                    }
                    new_frame = !second_field;
                }
                if key.is_some() {
                    self.last_slice = key;
                }
                self.seen_vcl = true;
                NalEvent {
                    starts_unit,
                    new_frame,
                    ..NalEvent::default()
                }
            }
            7 => {
                let mut sps_changed = false;
                if let Ok(sps) = h26x::h264::Sps::parse(&unescape_rbsp(&nal[1..])) {
                    sps_changed = self.sps264.get(&sps.id).is_some_and(|old| *old != sps);
                    if let Some(vui) = &sps.vui {
                        self.color = color_from(vui.colour_description, vui.full_range);
                        // H.264 ticks are fields: a frame is two.
                        self.frame_duration = vui
                            .timing
                            .filter(|&(n, t)| n > 0 && t > 0)
                            .map(|(n, t)| (n.saturating_mul(2), t));
                    } else {
                        self.color = ColorInfo::default();
                    }
                    self.sps264.insert(sps.id, sps);
                }
                NalEvent {
                    sps_changed,
                    ..self.opens_unit()
                }
            }
            8 => {
                let rbsp = unescape_rbsp(&nal[1..]);
                let sps = &self.sps264;
                if let Ok(pps) = h26x::h264::Pps::parse(&rbsp, &|id| sps.get(&id).cloned()) {
                    self.pps264.insert(pps.id, pps);
                }
                self.opens_unit()
            }
            6 | 9 | 14..=18 => self.opens_unit(),
            _ => NalEvent::default(),
        }
    }

    /// The start of an H.264 slice header, up to the picture order count
    /// fields (7.3.3), or `None` if its parameter sets are unknown.
    fn h264_slice_key(&self, nal: &[u8]) -> Option<SliceKey> {
        // Those fields are in the first few bytes; there is no need to
        // unescape the whole slice.
        let rbsp = unescape_rbsp(&nal[..nal.len().min(64)]);
        let mut r = Bits::new(rbsp.get(1..)?);
        let _first_mb_in_slice = r.ue()?;
        let _slice_type = r.ue()?;
        let pps_id = r.ue()?;
        let pps = self.pps264.get(&pps_id)?;
        let sps = self.sps264.get(&pps.sps_id)?;
        if sps.separate_colour_plane {
            r.u(2)?;
        }
        let frame_num = r.u(sps.log2_max_frame_num)?;
        let (mut field_pic, mut bottom_field) = (false, false);
        if !sps.frame_mbs_only {
            field_pic = r.flag()?;
            if field_pic {
                bottom_field = r.flag()?;
            }
        }
        let idr = nal[0] & 0x1f == 5;
        let idr_pic_id = if idr { r.ue()? } else { 0 };
        let (mut poc_lsb, mut delta_poc_bottom, mut delta_poc) = (0, 0, [0, 0]);
        if sps.poc_type == 0 {
            poc_lsb = r.u(sps.log2_max_poc_lsb)?;
            if pps.bottom_field_pic_order_in_frame_present && !field_pic {
                delta_poc_bottom = r.se()?;
            }
        }
        if sps.poc_type == 1 && !sps.delta_pic_order_always_zero {
            delta_poc[0] = r.se()?;
            if pps.bottom_field_pic_order_in_frame_present && !field_pic {
                delta_poc[1] = r.se()?;
            }
        }
        Some(SliceKey {
            frame_num,
            pps_id,
            field_pic,
            bottom_field,
            non_ref: nal[0] & 0x60 == 0,
            idr,
            idr_pic_id,
            poc_lsb,
            delta_poc_bottom,
            delta_poc,
        })
    }

    fn nal_hevc(&mut self, nal: &[u8]) -> NalEvent {
        let (Some(&b0), Some(&b1)) = (nal.first(), nal.get(1)) else {
            return NalEvent::default();
        };
        if ((b0 & 1) << 5) | (b1 >> 3) != 0 {
            // Another layer's: part of the access unit, nothing more.
            return NalEvent::default();
        }
        let t = (b0 >> 1) & 0x3f;
        match t {
            0..=31 => {
                let first = nal.get(2).is_some_and(|b| b & 0x80 != 0);
                let starts_unit = self.seen_vcl && first;
                let mut new_frame = false;
                if first {
                    if (16..=23).contains(&t) {
                        // BLA and IDR always start afresh; a CRA does where
                        // the stream starts (8.1.3: NoRaslOutputFlag).
                        self.skip_rasl = t <= 20 || self.stream_start;
                    }
                    let skipped = matches!(t, 8 | 9) && self.skip_rasl;
                    if !skipped {
                        self.stream_start = false;
                    }
                    new_frame = !skipped;
                }
                self.seen_vcl = true;
                NalEvent {
                    starts_unit,
                    new_frame,
                    ..NalEvent::default()
                }
            }
            33 => {
                let rbsp = unescape_rbsp(nal);
                if let Some(Ok(sps)) = rbsp.get(2..).map(h26x::hevc::Sps::parse) {
                    match &sps.vui {
                        Some(vui) => {
                            self.color = color_from(vui.colour_description, vui.full_range);
                            self.frame_duration = vui.timing.filter(|&(n, t)| n > 0 && t > 0);
                        }
                        None => self.color = ColorInfo::default(),
                    }
                }
                self.opens_unit()
            }
            32 | 34 | 35 | 39 | 41..=44 | 48..=55 => self.opens_unit(),
            36 | 37 => {
                // End of sequence or of bitstream: the next picture is an
                // IRAP that starts afresh.
                self.stream_start = true;
                NalEvent::default()
            }
            _ => NalEvent::default(),
        }
    }
}

fn color_from(description: Option<(u8, u8, u8)>, full_range: bool) -> ColorInfo {
    let (primaries, transfer, matrix) = description.unwrap_or((2, 2, 2));
    ColorInfo {
        matrix,
        primaries,
        transfer,
        full_range,
    }
}

/// A bit reader over an RBSP, for the few header fields read here; `None`
/// past the end.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Bits<'a> {
        Bits { data, pos: 0 }
    }

    fn flag(&mut self) -> Option<bool> {
        let byte = *self.data.get(self.pos / 8)?;
        let bit = byte >> (7 - self.pos % 8) & 1;
        self.pos += 1;
        Some(bit != 0)
    }

    fn u(&mut self, n: u32) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | self.flag()? as u32;
        }
        Some(v)
    }

    /// `ue(v)` (9.1).
    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while !self.flag()? {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        let rest = self.u(zeros)? as u64;
        u32::try_from((1u64 << zeros) - 1 + rest).ok()
    }

    /// `se(v)` (9.1.1).
    fn se(&mut self) -> Option<i32> {
        let k = self.ue()? as i64;
        Some(if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) } as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exp_golomb() {
        // 1 | 010 | 011 | 00100 | 00101: ue 0, 1, 2, 3, 4
        let mut r = Bits::new(&[0b1010_0110, 0b0100_0010, 0b1000_0000]);
        assert_eq!(
            (0..5).map(|_| r.ue().unwrap()).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4]
        );
        // se: 1 → 0, 010 → 1, 011 → -1, 00100 → 2
        let mut r = Bits::new(&[0b1010_0110, 0b0100_0000]);
        assert_eq!(
            (0..4).map(|_| r.se().unwrap()).collect::<Vec<_>>(),
            [0, 1, -1, 2]
        );
        assert_eq!(Bits::new(&[]).ue(), None);
    }

    #[test]
    fn labels() {
        assert_eq!(Codec::from_label("avc1"), Some(Codec::H264));
        assert_eq!(Codec::from_label("HEVC"), Some(Codec::Hevc));
        assert_eq!(Codec::from_label("h265"), Some(Codec::Hevc));
        assert_eq!(Codec::from_label("av1"), None);
    }

    #[test]
    fn sniffing() {
        // H.264: SPS, PPS, IDR slice headers.
        let h264 = [
            0, 0, 0, 1, 0x67, 0x64, 0, 0, 0, 1, 0x68, 0xee, 0, 0, 1, 0x65, 0x88,
        ];
        assert_eq!(sniff(&h264), Some(Codec::H264));
        // H.265: VPS, SPS, PPS, IDR_W_RADL.
        let hevc = [
            0, 0, 0, 1, 0x40, 0x01, 0x0c, 0, 0, 0, 1, 0x42, 0x01, 0x01, 0, 0, 0, 1, 0x44, 0x01,
            0xc0, 0, 0, 1, 0x26, 0x01, 0xaf,
        ];
        assert_eq!(sniff(&hevc), Some(Codec::Hevc));
        assert_eq!(sniff(b"DKIF not annex b"), None);
    }
}
