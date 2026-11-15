//! A decoded frame, packed for hand-off.
//!
//! The layout is wasm-av1's, which was upstream wasm-av1's `buffer_frame()`:
//! the three planes in one tightly packed allocation (Y, then Cb, then Cr,
//! no padding, 16-bit samples above 8 bits), with the geometry made
//! explicit: [`Frame::planes`] says where each plane starts, its stride, and
//! its size, so a consumer can build WebGL textures, a WebCodecs
//! `VideoFrame` (`format: "I420"` expects exactly this layout), or run the
//! RGBA conversion — without ever guessing at chroma subsampling.
//!
//! It is also exactly what rivet-h26x hands back ([`h26x::Picture`]: cropped,
//! packed, little-endian `u16` above 8 bits), so a frame *is* the decoder's
//! picture: [`FrameData`] keeps it, nothing is copied, and when the frame is
//! dropped the buffer goes back to the decoder's pool for a later picture.
//!
//! Sample size follows the *stream*: 8-bit streams give one byte per sample,
//! 9- to 16-bit streams give two (little-endian, the value in the low bits) —
//! i.e. `yuv420p` / `yuv420p10le` in ffmpeg's vocabulary. A stream whose luma
//! and chroma depths differ (an H.265 range-extensions corner) gives two
//! bytes for every plane and reports the wider depth.

use std::ops::Deref;

/// Chroma subsampling of a frame (`chroma_format_idc`'s numbering, which is
/// also wasm-av1's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PixelLayout {
    /// Monochrome: only a Y plane. The U/V entries in [`Frame::planes`] are empty.
    I400 = 0,
    /// 4:2:0 — chroma is half width and half height.
    I420 = 1,
    /// 4:2:2 — chroma is half width, full height.
    I422 = 2,
    /// 4:4:4 — chroma is full size.
    I444 = 3,
}

impl PixelLayout {
    /// Horizontal and vertical chroma subsampling shifts.
    pub fn subsampling(self) -> (u32, u32) {
        match self {
            PixelLayout::I400 => (0, 0),
            PixelLayout::I420 => (1, 1),
            PixelLayout::I422 => (1, 0),
            PixelLayout::I444 => (0, 0),
        }
    }

    pub fn has_chroma(self) -> bool {
        self != PixelLayout::I400
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => PixelLayout::I400,
            1 => PixelLayout::I420,
            2 => PixelLayout::I422,
            3 => PixelLayout::I444,
            _ => return None,
        })
    }
}

/// Where one plane lives inside [`Frame::data`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaneInfo {
    /// Byte offset of the first row.
    pub offset: usize,
    /// Bytes between the start of consecutive rows. Always exactly
    /// `width * bytes_per_sample` — planes are packed.
    pub stride: usize,
    /// Width in samples.
    pub width: usize,
    /// Height in rows.
    pub height: usize,
}

impl PlaneInfo {
    pub fn len(&self) -> usize {
        self.stride * self.height
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Colour metadata from the sequence parameter set's VUI, as ITU-T H.273
/// (ISO/IEC 23091-2) codes; `2` (unspecified) and limited range when the
/// stream does not say.
///
/// The conversion in [`crate::convert`] uses `matrix` and `full_range`;
/// primaries and transfer are carried for a renderer that wants to do HDR
/// tone mapping itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorInfo {
    /// Matrix coefficients (`1` = BT.709, `5`/`6` = BT.601, `9` = BT.2020 NCL, `2` = unspecified…).
    pub matrix: u8,
    /// Colour primaries code.
    pub primaries: u8,
    /// Transfer characteristics code.
    pub transfer: u8,
    /// True for full-range ("PC") YUV, false for limited ("TV") range.
    pub full_range: bool,
}

impl Default for ColorInfo {
    fn default() -> Self {
        ColorInfo {
            matrix: 2,
            primaries: 2,
            transfer: 2,
            full_range: false,
        }
    }
}

/// The samples of a frame: the decoder's own picture (handed back to its
/// buffer pool when dropped), or a plain buffer (tests, synthetic frames).
/// Dereferences to the packed bytes.
#[derive(Clone, Debug)]
pub struct FrameData(Storage);

#[derive(Clone, Debug)]
enum Storage {
    Owned(Vec<u8>),
    Picture(h26x::Picture),
}

impl FrameData {
    /// A picture of rivet-h26x's, kept as it is.
    pub fn from_picture(pic: h26x::Picture) -> FrameData {
        FrameData(Storage::Picture(pic))
    }
}

impl From<Vec<u8>> for FrameData {
    fn from(v: Vec<u8>) -> FrameData {
        FrameData(Storage::Owned(v))
    }
}

impl Deref for FrameData {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match &self.0 {
            Storage::Owned(v) => v,
            Storage::Picture(p) => p.packed(),
        }
    }
}

impl PartialEq for FrameData {
    fn eq(&self, other: &FrameData) -> bool {
        **self == **other
    }
}

/// A decoded, packed frame.
#[derive(Clone, Debug)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    /// Meaningful bits per sample: 8 to 16 (the wider of luma and chroma).
    pub bit_depth: u8,
    /// 1 for 8-bit streams, 2 above.
    pub bytes_per_sample: u8,
    pub layout: PixelLayout,
    /// Y, U, V in that order. U and V are zeroed for [`PixelLayout::I400`].
    pub planes: [PlaneInfo; 3],
    /// Presentation timestamp: the pts of the access unit the picture came
    /// in (container pts, the picture's index for an elementary stream, or
    /// what the caller passed to `push_access_unit`), or `None` if it had
    /// none.
    pub pts: Option<i64>,
    pub color: ColorInfo,
    /// The packed samples, `planes[0].len() + planes[1].len() + planes[2].len()` bytes.
    pub data: FrameData,
}

impl Frame {
    /// Compute the packed geometry for a frame of this shape.
    pub fn geometry(
        width: usize,
        height: usize,
        bytes_per_sample: usize,
        layout: PixelLayout,
    ) -> ([PlaneInfo; 3], usize) {
        let (ss_x, ss_y) = layout.subsampling();
        let luma = PlaneInfo {
            offset: 0,
            stride: width * bytes_per_sample,
            width,
            height,
        };
        let mut planes = [luma, PlaneInfo::default(), PlaneInfo::default()];
        let mut total = luma.len();
        if layout.has_chroma() {
            let cw = (width + (1usize << ss_x) - 1) >> ss_x;
            let ch = (height + (1usize << ss_y) - 1) >> ss_y;
            for plane in planes.iter_mut().skip(1) {
                *plane = PlaneInfo {
                    offset: total,
                    stride: cw * bytes_per_sample,
                    width: cw,
                    height: ch,
                };
                total += plane.len();
            }
        }
        (planes, total)
    }

    /// The bytes of one plane.
    pub fn plane(&self, index: usize) -> &[u8] {
        let p = &self.planes[index];
        &self.data[p.offset..p.offset + p.len()]
    }

    /// Bytes an RGBA rendering of this frame needs.
    pub fn rgba_len(&self) -> usize {
        self.width * self.height * 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_420_odd_dimensions_round_chroma_up() {
        let (p, total) = Frame::geometry(5, 3, 1, PixelLayout::I420);
        assert_eq!(
            p[0],
            PlaneInfo {
                offset: 0,
                stride: 5,
                width: 5,
                height: 3
            }
        );
        assert_eq!(
            p[1],
            PlaneInfo {
                offset: 15,
                stride: 3,
                width: 3,
                height: 2
            }
        );
        assert_eq!(
            p[2],
            PlaneInfo {
                offset: 21,
                stride: 3,
                width: 3,
                height: 2
            }
        );
        assert_eq!(total, 27);
    }

    #[test]
    fn geometry_hbd_doubles_strides() {
        let (p, total) = Frame::geometry(4, 2, 2, PixelLayout::I422);
        assert_eq!(p[0].stride, 8);
        assert_eq!(
            p[1],
            PlaneInfo {
                offset: 16,
                stride: 4,
                width: 2,
                height: 2
            }
        );
        assert_eq!(total, 16 + 8 + 8);
    }

    #[test]
    fn geometry_400_has_no_chroma() {
        let (p, total) = Frame::geometry(4, 4, 1, PixelLayout::I400);
        assert_eq!(total, 16);
        assert!(p[1].is_empty() && p[2].is_empty());
    }
}
