//! The decoder: an H.264 / H.265 decoder core driven the way upstream
//! wasm-av1 drove libaom.
//!
//! The core is rivet-h26x, rivet's own H.264 and H.265 decoders, behind
//! `src/core.rs`. This file is wasm-av1's, with an elementary stream where
//! wasm-av1 has IVF and access units where it has temporal units.
//!
//! Upstream's `AVX_Decoder` (`decode-av1.c`) is a pull model built for a
//! `requestAnimationFrame` loop: the page calls `run()` once per tick, which
//! reads *one* compressed frame from the source and decodes it into a ring of
//! up to `NUM_FRAMES_BUFFERED` (10) frames; `get_frame()` pops the oldest;
//! `video_finished()` says when the source is exhausted. That shape survives
//! here because it is the right one for a browser main thread — bounded work
//! per call, frames ready ahead of display — and it maps one-to-one onto the
//! wasm-bindgen surface in [`crate::wasm`].
//!
//! Input is an Annex-B elementary stream ([`Decoder::set_source_annexb`]), a
//! container file demuxed by rivet ([`Decoder::set_source_container`] —
//! MP4/fMP4/CMAF, Matroska/WebM, MPEG-TS; behind the `container` feature),
//! **or** access units pushed one at a time ([`Decoder::push_access_unit`]),
//! the escape hatch for a caller that already has samples in hand.
//!
//! [`Decoder::finished`] is true only when the source is exhausted *and*
//! every decoded frame has been popped from the ring. A picture of H.264 or
//! H.265 may be output only after later ones are decoded (B-pictures come
//! out of order, and the decoder knows a picture is complete when the next
//! begins), and with more than one thread several are decoded at once, so
//! at the end of the source [`Decoder::run`] first finishes what is still in
//! the decoder ([`Decoder::units_in_flight`]).

use std::collections::VecDeque;
use std::fmt;

use crate::annexb::{AnnexBError, AnnexBReader};
use crate::core::{self, Core, CoreError};
use crate::frame::Frame;
use crate::units::Codec;

#[cfg(feature = "container")]
use container::streaming::{DemuxHeader, StreamingDemuxer};

/// Upstream `NUM_FRAMES_BUFFERED`.
pub const DEFAULT_MAX_BUFFERED: usize = 10;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// How many decoded frames [`Decoder::run`] keeps ahead. Memory cost is
    /// `max_buffered` packed frames (1.5 bytes/pixel at 8-bit 4:2:0).
    pub max_buffered: usize,
    /// Threads the decoder core decodes with: several pictures at once
    /// (frame threading, both codecs), and the wavefront rows, tiles and
    /// slice segments of an H.265 picture in parallel. With more than one
    /// thread a unit's frame comes out a few units after it went in, or when
    /// the input ends (see [`Decoder::units_in_flight`]). Natively any
    /// number; on wasm only the `threads` build (atomics + shared memory)
    /// can run more than one and other builds force 1 — see
    /// [`crate::THREADS_SUPPORTED`]. With more than one thread the core
    /// blocks the calling thread while it waits for workers, so on wasm the
    /// decoder must live in a Worker.
    pub threads: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            max_buffered: DEFAULT_MAX_BUFFERED,
            threads: 1,
        }
    }
}

#[derive(Debug)]
pub enum Error {
    /// Not an elementary stream, or one of neither codec.
    AnnexB(AnnexBError),
    /// rivet could not demux the container, or its video track is neither
    /// H.264 nor H.265.
    #[cfg(feature = "container")]
    Container(String),
    /// The decoder could not be created. Nothing in rivet-h26x can fail
    /// there, so [`Decoder::new`] does not return this today; it is kept so
    /// that `new` can stay fallible.
    Open(CoreError),
    /// The core rejected part of an access unit (a malformed NAL unit, or a
    /// feature rivet-h26x does not implement). The decoder is still usable;
    /// the rest of the unit was decoded as far as it could be.
    Decode(CoreError),
    /// [`Decoder::run`] was called with no source and no pushed data.
    NoSource,
    /// An unknown codec label.
    UnknownCodec(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::AnnexB(e) => write!(f, "{e}"),
            #[cfg(feature = "container")]
            Error::Container(msg) => write!(f, "container: {msg}"),
            Error::Open(e) => write!(f, "could not open {}: {e}", core::NAME),
            Error::Decode(e) => write!(f, "{}: {e}", core::NAME),
            Error::NoSource => write!(
                f,
                "no source: call set_source_annexb, set_source_container or push_access_unit first"
            ),
            Error::UnknownCodec(label) => {
                write!(
                    f,
                    "codec {label:?} is neither H.264 (h264) nor H.265 (hevc)"
                )
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<AnnexBError> for Error {
    fn from(e: AnnexBError) -> Self {
        Error::AnnexB(e)
    }
}

/// What a call to [`Decoder::run`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    /// The ring was already full; nothing was read.
    Full,
    /// One access unit was consumed (and whatever frames it produced buffered).
    Consumed,
    /// No input was available: nothing has been pushed since the last call.
    Starved,
    /// The source is exhausted and everything it held is decoded; the ring
    /// may still hold frames.
    EndOfStream,
}

/// Counters for the stats overlay / tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub access_units_in: u64,
    pub bytes_in: u64,
    pub frames_out: u64,
    pub decode_errors: u64,
}

struct Pushed {
    data: Vec<u8>,
    pts: i64,
}

enum Source {
    AnnexB(AnnexBReader),
    #[cfg(feature = "container")]
    Container {
        demuxer: Box<dyn StreamingDemuxer>,
        header: DemuxHeader,
        /// `next_video_sample` returned `None`.
        exhausted: bool,
    },
    Push {
        queue: VecDeque<Pushed>,
        ended: bool,
    },
}

/// What a container told us about the stream — the subset of rivet's
/// `DemuxHeader` a player needs.
#[cfg(feature = "container")]
#[derive(Clone, Debug, PartialEq)]
pub struct ContainerInfo {
    /// rivet's codec label (`"h264"` or `"h265"`/`"hevc"` — anything else
    /// is rejected before you see it).
    pub codec: String,
    pub width: u32,
    pub height: u32,
    /// Ticks per second of the sample pts (MP4 track timescale, MKV 1e9, TS 90 kHz).
    pub timescale: u32,
    pub frame_rate: f64,
    pub duration_secs: f64,
    /// Sample count if the container knew it (0 otherwise).
    pub total_frames: u64,
}

#[cfg(feature = "container")]
impl ContainerInfo {
    fn from_header(header: &DemuxHeader) -> ContainerInfo {
        ContainerInfo {
            codec: header.codec.clone(),
            width: header.info.width,
            height: header.info.height,
            timescale: header.timescale,
            frame_rate: header.info.frame_rate,
            duration_secs: header.info.duration,
            total_frames: header.info.total_frames,
        }
    }
}

/// The H.264 / H.265 decoder with its ring of decoded frames.
pub struct Decoder {
    inner: Core,
    config: Config,
    source: Option<Source>,
    ring: VecDeque<Frame>,
    /// The frame most recently handed out by [`Decoder::next_frame`]; upstream
    /// kept `ad_LastFrame` so the JS side could keep reading it until the next call.
    current: Option<Frame>,
    /// Size of the most recent decoded frame — what `width()`/`height()`
    /// answer in push mode, where there is no container header to ask.
    last_size: Option<(usize, usize)>,
    /// Segment-fed mode: the initialisation segment prepended to each pushed
    /// media segment, and the timescale rivet reported for them.
    #[cfg(feature = "container")]
    init_segment: Option<Vec<u8>>,
    #[cfg(feature = "container")]
    segment_time_base: Option<(u32, u32)>,
    /// A new init segment replaced another (a rendition switch): the next
    /// segment's first H.265 unit gets an end of sequence in front — see
    /// [`Decoder::set_init_segment`].
    #[cfg(feature = "container")]
    new_sequence: bool,
    /// pts of the first and last sample of the segment most recently pushed.
    #[cfg(feature = "container")]
    last_segment_pts: Option<(i64, i64)>,
    /// pts of the last access unit handed to the core, any source.
    last_sent_pts: Option<i64>,
    stats: Stats,
}

/// The thread count a [`Config`] actually gets on this build.
pub fn effective_threads(config: &Config) -> u32 {
    if crate::THREADS_SUPPORTED {
        config.threads.max(1)
    } else {
        1
    }
}

/// The codec of a container's video track, or the error that it is neither.
#[cfg(feature = "container")]
fn container_codec(header: &DemuxHeader) -> Result<Codec, Error> {
    Codec::from_label(&header.codec).ok_or_else(|| {
        Error::Container(format!(
            "video track is {}, not H.264 or H.265",
            header.codec
        ))
    })
}

impl Decoder {
    pub fn new(config: Config) -> Result<Decoder, Error> {
        Ok(Decoder {
            inner: Core::open(&config),
            config,
            source: None,
            ring: VecDeque::with_capacity(config.max_buffered),
            current: None,
            last_size: None,
            #[cfg(feature = "container")]
            init_segment: None,
            #[cfg(feature = "container")]
            segment_time_base: None,
            #[cfg(feature = "container")]
            new_sequence: false,
            #[cfg(feature = "container")]
            last_segment_pts: None,
            last_sent_pts: None,
            stats: Stats::default(),
        })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// The codec of the stream, once a source or the first unit said.
    pub fn codec(&self) -> Option<Codec> {
        self.inner.codec()
    }

    /// Say which codec pushed access units are (`"h264"`/`"avc"`,
    /// `"h265"`/`"hevc"`); without it the first unit's NAL units decide.
    /// A change resets the decoder.
    pub fn set_codec(&mut self, label: &str) -> Result<(), Error> {
        let codec = Codec::from_label(label).ok_or_else(|| Error::UnknownCodec(label.into()))?;
        if self.inner.codec() != Some(codec) {
            self.reset_state()?;
            self.inner.set_codec(codec);
        }
        Ok(())
    }

    /// Use a whole Annex-B elementary stream as the source: H.264 or H.265
    /// as `codec` says, or as its first NAL units say when `None`. Replaces
    /// any previous source and resets the decoder.
    pub fn set_source_annexb(
        &mut self,
        data: Vec<u8>,
        codec: Option<Codec>,
    ) -> Result<&AnnexBReader, Error> {
        let reader = AnnexBReader::new(data, codec)?;
        self.reset_state()?;
        self.inner.set_codec(reader.codec());
        self.source = Some(Source::AnnexB(reader));
        match &self.source {
            Some(Source::AnnexB(r)) => Ok(r),
            _ => unreachable!(),
        }
    }

    /// The elementary stream, if that is the source.
    pub fn annexb(&self) -> Option<&AnnexBReader> {
        match &self.source {
            Some(Source::AnnexB(r)) => Some(r),
            _ => None,
        }
    }

    /// Use a whole container file (MP4/fMP4/CMAF, Matroska/WebM, TS) as the
    /// source, demuxed by rivet. Replaces any previous source and resets
    /// the decoder. Fails if rivet cannot read it or the video track is
    /// neither H.264 nor H.265.
    #[cfg(feature = "container")]
    pub fn set_source_container(&mut self, data: Vec<u8>) -> Result<ContainerInfo, Error> {
        let demuxer = container::streaming::demux_streaming_shared(bytes::Bytes::from(data))
            .map_err(|e| Error::Container(format!("{e:#}")))?;
        let header = demuxer.header().clone();
        let codec = container_codec(&header)?;
        self.reset_state()?;
        self.inner.set_codec(codec);
        let info = ContainerInfo::from_header(&header);
        self.source = Some(Source::Container {
            demuxer,
            header,
            exhausted: false,
        });
        Ok(info)
    }

    /// Segment-fed playback (CMAF / fMP4 as HLS or DASH serve it): remember
    /// the initialisation segment (`ftyp`+`moov`), so that each media segment
    /// (`moof`+`mdat`) pushed with [`Decoder::push_segment`] can be demuxed
    /// as `init ‖ segment` by rivet and its samples queued as access units —
    /// without resetting the decoder between segments. Switches to push mode
    /// (dropping an elementary-stream or container source, keeping
    /// queued/decoded frames).
    ///
    /// rivet hands each segment's first sample over with the parameter sets
    /// of the init segment's `avcC`/`hvcC` in front, so a new init segment
    /// (a rendition switch) needs nothing more: the decoder meets the new
    /// SPS and PPS before the segment's first picture. In H.265 that
    /// picture may be a CRA whose RASL pictures refer to pictures of the
    /// other rendition, which the decoder never had; an end of sequence in
    /// front of it makes the decoder start afresh there and skip them
    /// (8.1.3), so a second init segment puts one in front of the next
    /// segment. H.264 segments begin with an IDR picture and need none.
    #[cfg(feature = "container")]
    pub fn set_init_segment(&mut self, init: Vec<u8>) -> Result<(), Error> {
        if !matches!(self.source, Some(Source::Push { .. })) {
            self.reset_state()?;
            self.source = Some(Source::Push {
                queue: VecDeque::new(),
                ended: false,
            });
            self.init_segment = None;
        }
        self.new_sequence = self.init_segment.is_some();
        self.init_segment = Some(init);
        self.segment_time_base = None;
        Ok(())
    }

    /// Demux one media segment (with the init segment from
    /// [`Decoder::set_init_segment`] prepended; a self-contained file works
    /// too) and queue every video sample as an access unit with its pts.
    /// Returns how many were queued. The time base is the container's
    /// (see [`Decoder::time_base`]).
    #[cfg(feature = "container")]
    pub fn push_segment(&mut self, segment: &[u8]) -> Result<usize, Error> {
        if !matches!(self.source, Some(Source::Push { .. })) {
            self.reset_state()?;
            self.source = Some(Source::Push {
                queue: VecDeque::new(),
                ended: false,
            });
        }
        let mut whole =
            Vec::with_capacity(self.init_segment.as_ref().map_or(0, |i| i.len()) + segment.len());
        if let Some(init) = &self.init_segment {
            whole.extend_from_slice(init);
        }
        whole.extend_from_slice(segment);
        let mut demuxer = container::streaming::demux_streaming_shared(bytes::Bytes::from(whole))
            .map_err(|e| Error::Container(format!("{e:#}")))?;
        let header = demuxer.header().clone();
        let codec = container_codec(&header)?;
        if self.inner.codec() != Some(codec) {
            // The first segment, or another codec: either way the core
            // starts on this one. Units already queued are of the other.
            self.inner.set_codec(codec);
        }
        if header.timescale > 0 {
            self.segment_time_base = Some((1, header.timescale));
        }
        let mut n = 0;
        let mut first_last: Option<(i64, i64)> = None;
        loop {
            match demuxer.next_video_sample() {
                Ok(Some(s)) => {
                    first_last = Some(match first_last {
                        None => (s.pts_ticks, s.pts_ticks),
                        Some((f, l)) => (f.min(s.pts_ticks), l.max(s.pts_ticks)),
                    });
                    let mut data = s.data;
                    if std::mem::take(&mut self.new_sequence) && codec == Codec::Hevc {
                        // end_of_seq_rbsp (nal_unit_type 36, layer 0, tid 0)
                        data.splice(0..0, [0, 0, 0, 1, 0x48, 0x01]);
                    }
                    self.push_access_unit(data, s.pts_ticks)?;
                    n += 1;
                }
                Ok(None) => break,
                Err(e) => {
                    self.stats.decode_errors += 1;
                    return Err(Error::Container(format!("{e:#}")));
                }
            }
        }
        self.last_segment_pts = first_last;
        Ok(n)
    }

    /// pts of the first and last sample of the segment most recently pushed
    /// with [`Decoder::push_segment`] (a segment boundary, exactly, for
    /// [`Decoder::truncate_queued_from`]). With B-pictures the first in
    /// presentation order, which is also the segment's first in decode
    /// order (a segment begins with a random access point).
    #[cfg(feature = "container")]
    pub fn last_segment_pts(&self) -> Option<(i64, i64)> {
        self.last_segment_pts
    }

    /// The container's stream info, if the source is a container.
    #[cfg(feature = "container")]
    pub fn container_info(&self) -> Option<ContainerInfo> {
        match &self.source {
            Some(Source::Container { header, .. }) => Some(ContainerInfo::from_header(header)),
            _ => None,
        }
    }

    /// `(num, den)` such that `seconds = pts * num / den` for the pts on
    /// frames — seconds per frame from an elementary stream's VUI, or
    /// `(1, container timescale)`; `None` in push mode without segments
    /// (the caller knows its own units).
    pub fn time_base(&self) -> Option<(u32, u32)> {
        match &self.source {
            Some(Source::AnnexB(r)) => r.frame_duration(),
            #[cfg(feature = "container")]
            Some(Source::Container { header, .. }) => {
                (header.timescale > 0).then_some((1, header.timescale))
            }
            #[cfg(feature = "container")]
            Some(Source::Push { .. }) => self.segment_time_base,
            _ => None,
        }
    }

    /// Frames per second the stream's VUI declares (elementary streams and
    /// pushed units; a container's own rate is in [`ContainerInfo`]).
    pub fn vui_frame_rate(&self) -> Option<f64> {
        self.annexb()
            .and_then(AnnexBReader::frame_duration)
            .or_else(|| self.inner.frame_duration())
            .map(|(n, d)| d as f64 / n as f64)
    }

    /// Queue one access unit (the NAL units of one picture, Annex-B framed —
    /// start codes, parameter sets in band) with its pts. Switches the
    /// decoder to push mode; the first call after another source resets it.
    pub fn push_access_unit(&mut self, data: Vec<u8>, pts: i64) -> Result<(), Error> {
        if !matches!(self.source, Some(Source::Push { .. })) {
            self.reset_state()?;
            self.source = Some(Source::Push {
                queue: VecDeque::new(),
                ended: false,
            });
        }
        if let Some(Source::Push { queue, ended }) = &mut self.source {
            *ended = false;
            queue.push_back(Pushed { data, pts });
        }
        Ok(())
    }

    /// In push mode: no more access units will come, so [`Decoder::finished`]
    /// may become true once everything queued has been decoded and shown.
    pub fn end_of_stream(&mut self) {
        if let Some(Source::Push { ended, .. }) = &mut self.source {
            *ended = true;
        }
    }

    /// Access units queued in push mode and not yet sent to the decoder.
    pub fn pending_input(&self) -> usize {
        match &self.source {
            Some(Source::Push { queue, .. }) => queue.len(),
            _ => 0,
        }
    }

    /// Push mode: pts of the next queued access unit (the one the core
    /// gets next), if any is queued.
    pub fn next_queued_pts(&self) -> Option<i64> {
        match &self.source {
            Some(Source::Push { queue, .. }) => queue.front().map(|p| p.pts),
            _ => None,
        }
    }

    /// pts of the last access unit handed to the core, if any.
    pub fn last_sent_pts(&self) -> Option<i64> {
        self.last_sent_pts
    }

    /// Push mode: drop the queued (not yet decoded) access units whose pts
    /// is `>= pts`, keeping everything before. Returns how many were dropped.
    ///
    /// This is how a rendition switch stays seamless: the future the decoder
    /// has not reached is replaced — a new init segment and the other
    /// rendition's segments from a segment boundary at `pts` — with no flush,
    /// so nothing already decoded is lost and there is no gap on screen. The
    /// caller must make sure the core has not gone past `pts`
    /// ([`Decoder::next_queued_pts`] / [`Decoder::last_sent_pts`]) and that
    /// what it pushes next starts on a random access point (every CMAF
    /// segment does). With B-pictures a segment's later units have pts below
    /// its last: dropping by pts keeps exactly the units of the segments
    /// before the boundary, since a closed segment's pictures all show before
    /// the next segment's first.
    pub fn truncate_queued_from(&mut self, pts: i64) -> usize {
        match &mut self.source {
            Some(Source::Push { queue, ended }) => {
                let before = queue.len();
                queue.retain(|p| p.pts < pts);
                *ended = false;
                before - queue.len()
            }
            _ => 0,
        }
    }

    /// Drop everything decoded or queued and reset the core, e.g. for a seek.
    /// An elementary stream rewinds to its first access unit; a container
    /// source is forward-only in rivet and is dropped (set it again to
    /// replay).
    pub fn flush(&mut self) -> Result<(), Error> {
        self.reset_state()?;
        match &mut self.source {
            Some(Source::AnnexB(r)) => r.rewind(),
            #[cfg(feature = "container")]
            Some(Source::Container { .. }) => self.source = None,
            Some(Source::Push { queue, ended }) => {
                queue.clear();
                *ended = false;
            }
            None => {}
        }
        Ok(())
    }

    /// Start the core over: every decoded frame is dropped, and the
    /// reference pictures with them. On wasm the worker threads survive
    /// (they belong to a pool, not to the core), so a seek does not tear
    /// Workers down and spawn them again.
    fn reset_state(&mut self) -> Result<(), Error> {
        self.inner.flush();
        self.ring.clear();
        self.current = None;
        self.last_sent_pts = None;
        Ok(())
    }

    /// Frames decoded and waiting.
    pub fn frames_buffered(&self) -> usize {
        self.ring.len()
    }

    /// Access units the core has taken whose frames have not come out yet,
    /// with more than one thread. The core then decodes several pictures at
    /// once, away from the thread that calls [`Decoder::run`], so a `run`
    /// that returns [`RunOutcome::Starved`] may still have frames coming:
    /// while this is not 0, calling `run` again later collects those that
    /// are done without any new input. (Some may also be waiting for later
    /// pictures — B-pictures are output after the pictures they refer to —
    /// and come out only with more input or at the end.) Always 0 with one
    /// thread.
    pub fn units_in_flight(&self) -> usize {
        self.inner.in_flight()
    }

    fn source_done(&self) -> bool {
        match &self.source {
            None => false,
            Some(Source::AnnexB(r)) => r.is_empty(),
            #[cfg(feature = "container")]
            Some(Source::Container { exhausted, .. }) => *exhausted,
            Some(Source::Push { queue, ended }) => *ended && queue.is_empty(),
        }
    }

    /// True when there is nothing left to decode or show: the source is
    /// exhausted and every frame it produced has been popped.
    pub fn finished(&self) -> bool {
        self.source_done() && !self.inner.has_pictures() && self.ring.is_empty()
    }

    fn header_size(&self) -> Option<(usize, usize)> {
        match &self.source {
            #[cfg(feature = "container")]
            Some(Source::Container { header, .. }) => (header.info.width > 0
                && header.info.height > 0)
                .then_some((header.info.width as usize, header.info.height as usize)),
            _ => None,
        }
    }

    /// Stream width: the container header's, else the most recent decoded
    /// frame's. `None` until either is known.
    pub fn width(&self) -> Option<usize> {
        self.header_size()
            .map(|s| s.0)
            .or(self.last_size.map(|s| s.0))
    }

    /// Stream height, likewise.
    pub fn height(&self) -> Option<usize> {
        self.header_size()
            .map(|s| s.1)
            .or(self.last_size.map(|s| s.1))
    }

    /// Do a bounded slice of work — upstream `AVX_Decoder_run`.
    ///
    /// If the ring has room: decode the next access unit and move the
    /// frames that are ready into the ring. Call it once per animation
    /// frame, or in a loop until [`RunOutcome::Full`]/[`RunOutcome::EndOfStream`]
    /// to fill up.
    pub fn run(&mut self) -> Result<RunOutcome, Error> {
        if self.source.is_none() {
            return Err(Error::NoSource);
        }
        if self.ring.len() >= self.config.max_buffered {
            return Ok(RunOutcome::Full);
        }

        // Frames the last call could not fit into the ring come first.
        self.collect_pictures();
        if self.ring.len() >= self.config.max_buffered {
            return Ok(RunOutcome::Full);
        }

        // Next access unit from whichever source.
        let next: Option<(Vec<u8>, i64)> = match self.source.as_mut().unwrap() {
            Source::AnnexB(r) => r.next_unit().map(|u| (u.data.to_vec(), u.pts)),
            #[cfg(feature = "container")]
            Source::Container {
                demuxer, exhausted, ..
            } => match demuxer.next_video_sample() {
                Ok(Some(s)) => Some((s.data, s.pts_ticks)),
                Ok(None) => {
                    *exhausted = true;
                    None
                }
                Err(e) => {
                    // A damaged sample: report it; the demuxer stays usable.
                    self.stats.decode_errors += 1;
                    return Err(Error::Container(format!("{e:#}")));
                }
            },
            Source::Push { queue, .. } => queue.pop_front().map(|p| (p.data, p.pts)),
        };

        let Some((data, pts)) = next else {
            // No input. At the end of the source the pictures still in the
            // core are finished now, and their frames collected (as many
            // as the ring takes; the rest on the next calls): the end once
            // the core has none left. Otherwise starved.
            if !self.source_done() {
                return Ok(RunOutcome::Starved);
            }
            self.stats.decode_errors += self.inner.finish();
            self.collect_pictures();
            return Ok(if self.inner.has_pictures() {
                RunOutcome::Full
            } else {
                RunOutcome::EndOfStream
            });
        };

        self.stats.access_units_in += 1;
        self.stats.bytes_in += data.len() as u64;
        self.last_sent_pts = Some(pts);
        let sent = self.inner.send(&data, pts);
        self.collect_pictures();
        if let Err(e) = sent {
            // What could be decoded of the unit was; the decoder itself is
            // fine to continue.
            self.stats.decode_errors += 1;
            return Err(Error::Decode(e));
        }
        Ok(RunOutcome::Consumed)
    }

    /// Move the frames the core has ready into the ring, up to capacity; the
    /// rest stay in the core for the next call.
    fn collect_pictures(&mut self) {
        while self.ring.len() < self.config.max_buffered {
            let Some(frame) = self.inner.picture() else {
                break;
            };
            self.last_size = Some((frame.width, frame.height));
            self.ring.push_back(frame);
            self.stats.frames_out += 1;
        }
        self.stats.decode_errors += self.inner.take_dropped();
    }

    /// Pop the oldest buffered frame — upstream `AVX_Decoder_get_frame`.
    ///
    /// The returned reference stays valid (as [`Decoder::current_frame`])
    /// until the next call to this method or to [`Decoder::flush`].
    pub fn next_frame(&mut self) -> Option<&Frame> {
        self.current = self.ring.pop_front();
        self.current.as_ref()
    }

    /// The oldest buffered frame — what the next [`Decoder::next_frame`] will
    /// return — without popping it. Lets a caller pace on its pts first.
    pub fn peek_next(&self) -> Option<&Frame> {
        self.ring.front()
    }

    /// The frame most recently returned by [`Decoder::next_frame`].
    pub fn current_frame(&self) -> Option<&Frame> {
        self.current.as_ref()
    }

    /// Take the current frame out, leaving none.
    pub fn take_current_frame(&mut self) -> Option<Frame> {
        self.current.take()
    }

    /// Decode everything available right now, filling the ring.
    pub fn run_until_full(&mut self) -> Result<RunOutcome, Error> {
        loop {
            match self.run()? {
                RunOutcome::Consumed => continue,
                other => return Ok(other),
            }
        }
    }
}

impl fmt::Debug for Decoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Decoder")
            .field("codec", &self.inner.codec())
            .field("buffered", &self.ring.len())
            .field("finished", &self.finished())
            .field("stats", &self.stats)
            .finish()
    }
}
