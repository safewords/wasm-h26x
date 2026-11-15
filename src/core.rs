//! The decoder core: rivet's own H.264 and H.265 decoders (`rivet-h26x`,
//! imported as `h26x`).
//!
//! Besides the thread hook in `threads.rs`, the one place that names them.
//! rivet-h26x takes NAL units in decode order and hands back cropped,
//! packed pictures in output order — packed the way [`Frame`] is (Y, Cb, Cr
//! one after another, no padding, little-endian `u16` above 8 bits) — so
//! this is a thin adapter: it splits each access unit into its NAL units,
//! keeps the frames that are decoded until [`crate::decoder`] pulls them,
//! and gives each the pts it belongs to.
//!
//! **Timestamps.** A picture leaves the decoder in output order, which in a
//! conforming stream is presentation order, so the frame that comes out
//! next is always the one with the earliest pts not given out yet: the pts
//! of each access unit go into a sorted queue, and each frame takes the
//! smallest. That needs every pts to have a frame, so the units whose
//! pictures are not output on their own — a second field, a skipped RASL
//! picture — put none in ([`crate::units`]). The colour description of the
//! sequence parameter set in force when the unit went in travels with its
//! pts.
//!
//! **Threads.** With one thread the decoder works on the caller's thread,
//! as each NAL unit is pushed; a picture is complete once the next one has
//! begun (or the input ends), and leaves when the reorder buffer lets it.
//! With more, rivet-h26x decodes several pictures at once (frame threading)
//! and the rows of a picture in parallel (H.265 wavefronts, tiles and slice
//! segments) on a pool of workers; a picture then comes out a few units
//! later, or when the input ends ([`Core::finish`]).
//!
//! **Resets.** rivet-h26x has no reset: [`Core::flush`] drops the decoder
//! and the next unit makes a new one. On wasm its pool's threads come back
//! to the [`crate::threads`] pool, so a seek starts no new Workers.

use std::collections::VecDeque;

use crate::decoder::Config;
use crate::frame::{ColorInfo, Frame, FrameData, PixelLayout, PlaneInfo};
use crate::units::{Codec, Tracker};

/// What the decoder core reports when it rejects data: rivet-h26x's error
/// (`Bitstream` or `Unsupported`, each with a message).
pub type CoreError = h26x::Error;

/// The name the core goes by in error messages.
pub(crate) const NAME: &str = "rivet-h26x";

/// The decoder of one codec.
enum Dec {
    H264(Box<h26x::h264::H264Decoder>),
    Hevc(Box<h26x::hevc::HevcDecoder>),
}

impl Dec {
    fn new(codec: Codec, threads: usize) -> Dec {
        match codec {
            Codec::H264 => Dec::H264(Box::new(h26x::h264::H264Decoder::with_threads(threads))),
            Codec::Hevc => Dec::Hevc(Box::new(h26x::hevc::HevcDecoder::with_threads(threads))),
        }
    }

    fn push_nal(&mut self, nal: &[u8]) -> h26x::Result<()> {
        match self {
            Dec::H264(d) => d.push_nal(nal),
            Dec::Hevc(d) => d.push_nal(nal),
        }
    }

    fn try_next_picture(&mut self) -> Option<h26x::Picture> {
        match self {
            Dec::H264(d) => d.try_next_picture(),
            Dec::Hevc(d) => d.try_next_picture(),
        }
    }

    fn next_picture(&mut self) -> Option<h26x::Picture> {
        match self {
            Dec::H264(d) => d.next_picture(),
            Dec::Hevc(d) => d.next_picture(),
        }
    }

    fn flush(&mut self) -> h26x::Result<()> {
        match self {
            Dec::H264(d) => d.flush(),
            Dec::Hevc(d) => d.flush(),
        }
    }
}

/// A pts waiting for its frame, with the colour description it came under.
#[derive(Clone, Copy, Debug)]
struct Pending {
    pts: i64,
    color: ColorInfo,
}

pub(crate) struct Core {
    threads: usize,
    codec: Option<Codec>,
    dec: Option<Dec>,
    tracker: Option<Tracker>,
    /// The pts of units whose frames have not come out, ascending.
    pending: VecDeque<Pending>,
    /// Frames decoded, not yet pulled.
    ready: VecDeque<Frame>,
    /// Errors met while frames were being collected by [`Core::picture`],
    /// not yet counted.
    dropped: u64,
}

impl Core {
    pub(crate) fn open(config: &Config) -> Core {
        let threads = crate::decoder::effective_threads(config) as usize;
        #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
        if threads > 1 {
            crate::threads::install();
        }
        Core {
            threads,
            codec: None,
            dec: None,
            tracker: None,
            pending: VecDeque::new(),
            ready: VecDeque::new(),
            dropped: 0,
        }
    }

    /// The codec of the stream, if known yet.
    pub(crate) fn codec(&self) -> Option<Codec> {
        self.codec
    }

    /// Say which codec comes next. A change starts the core over.
    pub(crate) fn set_codec(&mut self, codec: Codec) {
        if self.codec != Some(codec) {
            self.flush();
            self.codec = Some(codec);
            self.tracker = Some(Tracker::new(codec));
        }
    }

    /// Forget the stream: reference pictures, anything in flight, or
    /// decoded and not yet pulled. The codec and the parameter sets seen
    /// stay; the decoder itself is made again by the next unit.
    pub(crate) fn flush(&mut self) {
        self.dec = None;
        if let Some(t) = self.tracker.as_mut() {
            t.restart();
        }
        self.pending.clear();
        self.ready.clear();
        self.dropped = 0;
    }

    /// Decode one access unit (NAL units behind start codes) whose frame
    /// has the presentation time `pts`. Its frame comes out of
    /// [`Core::picture`] once it is decoded and every frame shown before it
    /// has come out. An error is the first NAL unit the decoder refused;
    /// the rest of the unit is still pushed, and decoding goes on.
    pub(crate) fn send(&mut self, data: &[u8], pts: i64) -> Result<(), CoreError> {
        if self.codec.is_none() {
            let codec = crate::units::sniff(data).ok_or_else(|| {
                h26x::Error::Bitstream(
                    "cannot tell whether this is H.264 or H.265; give the codec".into(),
                )
            })?;
            self.set_codec(codec);
        }
        let codec = self.codec.expect("set above");
        let threads = self.threads;
        let dec = self.dec.get_or_insert_with(|| Dec::new(codec, threads));
        let tracker = self.tracker.as_mut().expect("set with the codec");
        let mut frames = 0;
        let mut first_error = None;
        for nal in h26x::nal::annexb_nals(data) {
            let ev = tracker.nal(nal);
            if ev.new_frame {
                frames += 1;
            }
            if ev.sps_changed {
                // A new H.264 sequence (another rendition, a new size): its
                // IDR picture will output every picture still waiting, and
                // rivet-h26x would crop them with the new SPS's cropping
                // window — the DPB keeps one window, not one per picture.
                // Output them now, under the window they were decoded with.
                if let Err(e) = dec.flush() {
                    first_error.get_or_insert(e);
                }
            }
            if let Err(e) = dec.push_nal(nal) {
                first_error.get_or_insert(e);
            }
        }
        let color = tracker.color();
        for _ in 0..frames {
            let at = self.pending.partition_point(|p| p.pts <= pts);
            self.pending.insert(at, Pending { pts, color });
        }
        self.collect(false);
        first_error.map_or(Ok(()), Err)
    }

    /// Moves the pictures that are finished to `ready` (all of them, if
    /// `wait`: decoding goes on, on this thread too, until they are).
    fn collect(&mut self, wait: bool) {
        let Some(dec) = self.dec.as_mut() else {
            return;
        };
        loop {
            let pic = if wait {
                dec.next_picture()
            } else {
                dec.try_next_picture()
            };
            let Some(pic) = pic else { break };
            let p = self.pending.pop_front();
            let color = p.map_or_else(
                || {
                    self.tracker
                        .as_ref()
                        .map(Tracker::color)
                        .unwrap_or_default()
                },
                |p| p.color,
            );
            self.ready
                .push_back(frame_from_picture(pic, p.map(|p| p.pts), color));
        }
    }

    /// The end of the input: every picture still in the decoder is finished
    /// and output before this returns, and waits in [`Core::picture`].
    /// Returns how many errors the decoder reported doing so (0 or 1).
    pub(crate) fn finish(&mut self) -> u64 {
        let Some(dec) = self.dec.as_mut() else {
            self.pending.clear();
            return 0;
        };
        let failed = u64::from(dec.flush().is_err());
        self.collect(true);
        // Pts whose pictures were never output (refused, or lost): nothing
        // will come for them now.
        self.pending.clear();
        failed
    }

    /// The next decoded frame, `None` when none is ready: all have been
    /// pulled, or the next is still being decoded (or waits for later
    /// pictures before it may be output).
    pub(crate) fn picture(&mut self) -> Option<Frame> {
        if self.ready.is_empty() {
            self.collect(false);
        }
        self.ready.pop_front()
    }

    /// Errors met while collecting frames, since the last call. (rivet-h26x
    /// reports errors when data goes in, so this is 0 today; kept for the
    /// shape of [`crate::decoder`].)
    pub(crate) fn take_dropped(&mut self) -> u64 {
        std::mem::take(&mut self.dropped)
    }

    /// Frames whose access units went in and that have not come out yet,
    /// with more than one thread: being decoded on the pool, or waiting in
    /// the reorder buffer for later pictures. While it is not 0, frames may
    /// come out with no new input. Always 0 with one thread, where nothing
    /// is decoded but what the caller pushes.
    pub(crate) fn in_flight(&self) -> usize {
        if self.threads > 1 {
            self.pending.len()
        } else {
            0
        }
    }

    /// True while frames are decoded and not pulled, or units went in whose
    /// frames have not come out yet.
    pub(crate) fn has_pictures(&self) -> bool {
        !self.ready.is_empty() || !self.pending.is_empty()
    }

    /// Seconds per frame from the stream's VUI, if it says.
    pub(crate) fn frame_duration(&self) -> Option<(u32, u32)> {
        self.tracker.as_ref().and_then(Tracker::frame_duration)
    }
}

/// Take a decoded rivet-h26x picture as a frame: the picture moves in,
/// nothing is copied.
fn frame_from_picture(pic: h26x::Picture, pts: Option<i64>, color: ColorInfo) -> Frame {
    let layout = match pic.chroma {
        h26x::ChromaFormat::Monochrome => PixelLayout::I400,
        h26x::ChromaFormat::Yuv420 => PixelLayout::I420,
        h26x::ChromaFormat::Yuv422 => PixelLayout::I422,
        h26x::ChromaFormat::Yuv444 => PixelLayout::I444,
    };
    let bps = pic.bytes_per_sample();
    let mut planes = [PlaneInfo::default(); 3];
    for (i, p) in pic.planes.iter().take(3).enumerate() {
        planes[i] = PlaneInfo {
            offset: p.offset,
            stride: p.width as usize * bps,
            width: p.width as usize,
            height: p.height as usize,
        };
    }
    Frame {
        width: pic.width as usize,
        height: pic.height as usize,
        bit_depth: pic.bit_depth.max(pic.bit_depth_chroma) as u8,
        bytes_per_sample: bps as u8,
        layout,
        planes,
        pts,
        color,
        data: FrameData::from_picture(pic),
    }
}
