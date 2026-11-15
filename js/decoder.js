// A JS-shaped wrapper around the wasm `H26xDecoder` (wasm-av1's `Decoder`,
// with an Annex-B elementary stream where wasm-av1 takes IVF, and access
// units where it takes temporal units).
//
// The wasm object speaks in pointers and lengths; this class turns the current
// frame into a `Frame` with typed-array views, and enforces the one rule those
// views come with: they are valid only until the next call that can decode
// (memory may grow and detach them). A `Frame` knows its generation and throws
// if you touch it after it has been superseded, rather than handing you a
// detached buffer.

/** Pixel layouts, matching the wasm side (`chroma_format_idc`'s numbering, and wasm-av1's). */
export const Layout = Object.freeze({ I400: 0, I420: 1, I422: 2, I444: 3 });

/** ISO 23091-2 matrix codes we know how to convert. */
export const Matrix = Object.freeze({ IDENTITY: 0, BT709: 1, UNSPECIFIED: 2, FCC: 4, BT470BG: 5, BT601: 6, SMPTE240: 7, BT2020_NCL: 9, BT2020_CL: 10 });

/**
 * Kr/Kb for a matrix code — the same table as `convert.rs`, so the WebGL
 * shader and the wasm converter agree on colour.
 */
export function krKbFor(matrix, height) {
  switch (matrix) {
    case 1: return [0.2126, 0.0722];
    case 4: return [0.30, 0.11];
    case 5:
    case 6: return [0.299, 0.114];
    case 7: return [0.212, 0.087];
    case 9:
    case 10: return [0.2627, 0.0593];
    default: return height > 576 ? [0.2126, 0.0722] : [0.299, 0.114];
  }
}

export class Frame {
  /** @package */
  constructor(decoder, generation) {
    this._dec = decoder;
    this._gen = generation;
    const raw = decoder.raw;
    this.width = raw.frameWidth();
    this.height = raw.frameHeight();
    this.bitDepth = raw.frameBitDepth();
    this.bytesPerSample = raw.frameBytesPerSample();
    this.layout = raw.frameLayout();
    const pts = raw.framePts();
    /** pts of the access unit the picture came in (presentation order), or null. */
    this.pts = Number.isNaN(pts) ? null : pts;
    this.matrix = raw.frameMatrix();
    this.primaries = raw.framePrimaries();
    this.transfer = raw.frameTransfer();
    this.fullRange = raw.frameFullRange();
    /** [{offset, stride, width, height}] for Y, U, V (U/V zero for I400). */
    this.planes = [0, 1, 2].map((i) => ({
      offset: raw.planeOffset(i),
      stride: raw.planeStride(i),
      width: raw.planeWidth(i),
      height: raw.planeHeight(i),
    }));
    this._ptr = raw.framePtr();
    this._len = raw.frameLen();
    /** Wall-clock time in seconds this frame is due, if the decoder has a time base. */
    this.seconds = decoder.secondsOf(this.pts);
  }

  /** True until the decoder moved on to another frame. */
  get valid() {
    return this._gen === this._dec._generation;
  }

  _check() {
    if (!this.valid) throw new Error('wasm-h26x: frame is stale (nextFrame() or flush() was called since)');
  }

  /** Transient view over the packed planes in wasm memory. Rebuild after any decode call. */
  get data() {
    this._check();
    return new Uint8Array(this._dec.rt.wasm.memory.buffer, this._ptr, this._len);
  }

  /** Transient view over one plane (0 Y, 1 U, 2 V). */
  plane(i) {
    const p = this.planes[i];
    return this.data.subarray(p.offset, p.offset + p.stride * p.height);
  }

  /** Copy the packed planes out of wasm memory (safe to keep / transfer). */
  copyData() {
    return this.data.slice();
  }

  /**
   * Convert to RGBA8 in wasm (SIMD128 in the SIMD build) and return a
   * transient `Uint8ClampedArray` view of `width*height*4` bytes.
   */
  rgba() {
    this._check();
    const ptr = this._dec.raw.convertToRgba();
    return new Uint8ClampedArray(this._dec.rt.wasm.memory.buffer, ptr, this.width * this.height * 4);
  }

  /** A fresh ImageData holding a copy of the RGBA conversion. */
  toImageData() {
    return new ImageData(this.rgba().slice(), this.width, this.height);
  }

  /** Copy RGBA into an existing ImageData of the same size (no allocation). */
  writeImageData(imageData) {
    imageData.data.set(this.rgba());
    return imageData;
  }

  /**
   * A structured-clone-friendly copy of everything, for posting from a
   * Worker: `{...meta, data: ArrayBuffer[, rgba: ArrayBuffer]}`.
   */
  toTransferable({ rgba = false } = {}) {
    const out = {
      width: this.width, height: this.height, bitDepth: this.bitDepth, bytesPerSample: this.bytesPerSample,
      layout: this.layout, pts: this.pts, seconds: this.seconds, matrix: this.matrix, primaries: this.primaries,
      transfer: this.transfer, fullRange: this.fullRange, planes: this.planes,
      data: this.copyData().buffer,
    };
    if (rgba) out.rgba = this.rgba().slice().buffer;
    return out;
  }
}

function asU8(bytes) {
  return bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
}

/**
 * 'annexb' | 'mp4' | 'mkv' | 'ts' | 'unknown' from the first bytes of a file.
 * `annexb` is an elementary stream (`.264`, `.265`): zero bytes, then 0x01.
 * (`'webm'` is what wasm-av1 calls Matroska; here it is `'mkv'`.)
 */
export function sniff(bytes) {
  const b = asU8(bytes);
  let z = 0;
  while (z < b.length && z < 64 && b[z] === 0) z++;
  if (z >= 2 && b[z] === 1) return 'annexb';
  if (b.length >= 8 && b[4] === 0x66 && b[5] === 0x74 && b[6] === 0x79 && b[7] === 0x70) return 'mp4'; // ....ftyp
  if (b.length >= 8 && b[4] === 0x6d && b[5] === 0x6f && b[6] === 0x6f && b[7] === 0x66) return 'mp4'; // a bare moof segment
  if (b.length >= 8 && b[4] === 0x73 && b[5] === 0x74 && b[6] === 0x79 && b[7] === 0x70) return 'mp4'; // a styp-led media segment
  if (b.length >= 4 && b[0] === 0x1a && b[1] === 0x45 && b[2] === 0xdf && b[3] === 0xa3) return 'mkv'; // EBML
  if (b.length >= 189 && b[0] === 0x47 && b[188] === 0x47) return 'ts';
  return 'unknown';
}

/** Outcomes of `run()`; mirrors the wasm `RunResult`. */
export const Run = Object.freeze({ FULL: 0, CONSUMED: 1, STARVED: 2, END_OF_STREAM: 3 });

export class Decoder {
  /**
   * @param {import('./loader.js').Runtime} runtime  from `loadWasmH26x()`
   * @param {object} [opts]
   * @param {number} [opts.maxBuffered=10]  decoded frames kept ahead
   * @param {number} [opts.threads=1]  decoder threads; more than 1 only on a `threads` runtime
   *                                   (`runtime.threads`), and only from inside a Worker — see `threads` for what applied
   * @param {'h264'|'hevc'} [opts.codec]  for pushed access units; otherwise their NAL units say
   */
  constructor(runtime, { maxBuffered = 10, threads = 1, codec } = {}) {
    this.rt = runtime;
    this.raw = new runtime.mod.H26xDecoder(maxBuffered, Math.max(1, threads | 0), codec ?? undefined);
    this.maxBuffered = maxBuffered;
    /** Worker threads actually running (1 unless the runtime has threads and more were asked). */
    this.threads = typeof this.raw.threads === 'function' ? this.raw.threads() : 1;
    this._generation = 0;
    this._current = null;
    /** Seconds per pts tick: from the stream's VUI frame rate or the container's timescale, or `setTimeBase()` in push mode. */
    this.timeBase = null;
  }

  /**
   * Load a whole Annex-B elementary stream (`.264`/`.265`; ArrayBuffer or
   * Uint8Array). `codec` is 'h264' or 'hevc', or omitted to tell from the NAL
   * units. Resets the decoder. Frames are numbered 0, 1, 2… in presentation
   * order; `timeBase` is the VUI's frame duration if the stream has one.
   */
  setSourceAnnexB(bytes, codec) {
    this._bump();
    this.raw.setSourceAnnexB(asU8(bytes), codec ?? undefined);
    this._readTimeBase();
    return this.info();
  }

  /**
   * Load a whole container file — MP4 / fragmented MP4 (CMAF), Matroska,
   * MPEG-TS — demuxed by rivet inside the wasm. Resets the decoder. Throws if
   * the build lacks container support or the video track is neither H.264
   * nor H.265.
   */
  setSourceContainer(bytes) {
    this._bump();
    this.raw.setSourceContainer(asU8(bytes));
    this._readTimeBase();
    return this.info();
  }

  /** Elementary stream or container, decided by the file's first bytes. */
  setSource(bytes, codec) {
    return sniff(asU8(bytes)) === 'annexb' ? this.setSourceAnnexB(bytes, codec) : this.setSourceContainer(bytes);
  }

  /** 'h264', 'hevc', or null while unknown. */
  get codec() {
    return this.raw.codec() || null;
  }

  /** Say which codec pushed access units are ('h264' | 'hevc'). A change resets the decoder. */
  setCodec(codec) {
    this._bump();
    this.raw.setCodec(codec);
  }

  _readTimeBase() {
    const num = this.raw.timeBaseNum();
    const den = this.raw.timeBaseDen();
    this.timeBase = num && den ? num / den : null;
  }

  /**
   * Push mode: one access unit — the NAL units of one picture, Annex-B framed
   * (start codes, parameter sets in band) — and its presentation pts. Frames
   * come out in presentation order, each with its own unit's pts.
   */
  pushAccessUnit(bytes, pts = 0) {
    this.raw.pushAccessUnit(asU8(bytes), pts);
  }

  /** wasm-av1's name for `pushAccessUnit`, so code written against it runs unchanged. */
  pushTemporalUnit(bytes, pts = 0) {
    this.pushAccessUnit(bytes, pts);
  }

  /**
   * Segment-fed playback (HLS/DASH CMAF): give the initialisation segment
   * once, then `pushSegment()` each media segment. Frames flow continuously
   * across segments; pts are in the container's timescale, which becomes
   * `timeBase` after the first segment.
   */
  setInitSegment(bytes) {
    this.raw.setInitSegment(asU8(bytes));
    this.timeBase = null;
  }

  /** Demux one media segment in wasm (rivet) and queue its samples. Returns the sample count. */
  pushSegment(bytes) {
    const n = this.raw.pushSegment(asU8(bytes));
    this._readTimeBase();
    return n;
  }

  /** pts of the first and last sample of the segment most recently pushed — a segment boundary, exactly. */
  lastSegmentRange() {
    const firstPts = this.raw.lastSegmentFirstPts();
    const lastPts = this.raw.lastSegmentLastPts();
    return { firstPts: Number.isNaN(firstPts) ? null : firstPts, lastPts: Number.isNaN(lastPts) ? null : lastPts };
  }

  /** Push mode: pts of the next queued (not yet decoded) access unit, or null. */
  nextQueuedPts() {
    const p = this.raw.nextQueuedPts();
    return Number.isNaN(p) ? null : p;
  }

  /** pts of the last access unit handed to the decoder, or null. */
  lastSentPts() {
    const p = this.raw.lastSentPts();
    return Number.isNaN(p) ? null : p;
  }

  /** Push mode: drop queued, not-yet-decoded access units with pts ≥ `pts`. Returns how many. */
  truncateQueuedFrom(pts) {
    return this.raw.truncateQueuedFrom(pts);
  }

  /**
   * Seamless rendition switch: replace the not-yet-decoded future from a
   * segment boundary. If the decoder has not been handed anything at or past
   * `boundaryPts` (null = nothing beyond the boundary was pushed yet), the
   * queue is truncated there, `init` becomes the init segment for what is
   * pushed next (the other rendition's segments from that boundary), and
   * decoding continues without a flush — no gap. Returns `{ok, dropped}`;
   * `ok: false` means the boundary is already behind the decoder: pick the
   * next one. `HlsH26xVideo.selectVariant()` drives this.
   */
  switchStream({ boundaryPts = null, init }) {
    if (boundaryPts != null) {
      const front = this.nextQueuedPts();
      const last = this.lastSentPts();
      const passed = front != null ? front > boundaryPts : (last != null && last >= boundaryPts);
      if (passed) return { ok: false, dropped: 0, front, last };
    }
    const dropped = boundaryPts != null ? this.truncateQueuedFrom(boundaryPts) : 0;
    this.setInitSegment(init);
    return { ok: true, dropped };
  }

  /** Push mode: how to turn pts into seconds (e.g. `1 / track.timescale`). */
  setTimeBase(secondsPerTick) {
    this.timeBase = secondsPerTick;
  }

  endOfStream() {
    this.raw.endOfStream();
  }

  /** Reset everything; elementary streams rewind. */
  flush() {
    this._bump();
    this.raw.flush();
  }

  /** One bounded step; returns a `Run` value. Throws on a rejected access unit — keep going. */
  run() {
    return this.raw.run();
  }

  runUntilFull() {
    return this.raw.runUntilFull();
  }

  get framesBuffered() {
    return this.raw.framesBuffered();
  }

  /**
   * Access units the decoder has taken whose frames have not come out yet.
   * With threads it decodes several pictures at once, off the calling
   * thread, so `run()` can return `Run.STARVED` with frames still coming:
   * while this is not 0, call `run()` again a little later. (Some may be
   * waiting for later pictures — a B-picture's references are output after
   * it is decoded — and come out only with more input or at the end.)
   * Always 0 with one thread.
   */
  get unitsInFlight() {
    return this.raw.unitsInFlight?.() ?? 0;
  }

  get finished() {
    return this.raw.finished();
  }

  get pendingInput() {
    return this.raw.pendingInput();
  }

  info() {
    return {
      codec: this.codec,
      width: this.raw.width(),
      height: this.raw.height(),
      timeBase: this.timeBase,
      frameCount: this.raw.frameCountHint() || null,
      frameRate: this.raw.frameRateHint() || null,
      duration: this.raw.durationHint() || null,
    };
  }

  stats() {
    const s = this.raw.stats();
    const out = { accessUnitsIn: s.accessUnitsIn, bytesIn: s.bytesIn, framesOut: s.framesOut, decodeErrors: s.decodeErrors };
    s.free?.();
    return out;
  }

  secondsOf(pts) {
    return pts == null || this.timeBase == null ? null : pts * this.timeBase;
  }

  /** pts of the frame `nextFrame()` would return next, or null (nothing buffered / no pts). Does not pop. */
  peekPts() {
    const p = this.raw.peekPts();
    return Number.isNaN(p) ? null : p;
  }

  /** Seconds of the next frame, or null. */
  peekSeconds() {
    return this.secondsOf(this.peekPts());
  }

  /** Pop the oldest buffered frame, or null. Invalidates the previous Frame. */
  nextFrame() {
    this._bump();
    if (!this.raw.nextFrame()) return null;
    this._current = new Frame(this, this._generation);
    return this._current;
  }

  /** The frame most recently returned by nextFrame(), if still valid. */
  get currentFrame() {
    return this._current && this._current.valid ? this._current : null;
  }

  _bump() {
    this._generation++;
    this._current = null;
  }

  free() {
    this._bump();
    this.raw.free();
  }
}
