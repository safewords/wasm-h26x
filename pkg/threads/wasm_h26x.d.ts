/* tslint:disable */
/* eslint-disable */

/**
 * Decoder counters.
 */
export class DecoderStats {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    accessUnitsIn: number;
    bytesIn: number;
    decodeErrors: number;
    framesOut: number;
}

/**
 * One H.264 / H.265 decoder with its frame ring and a reusable RGBA
 * scratch buffer.
 */
export class H26xDecoder {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * `"h264"`, `"hevc"`, or `""` while unknown.
     */
    codec(): string;
    /**
     * Convert the current frame to RGBA8 into the internal scratch buffer
     * (SIMD128 in the SIMD build) and return its pointer; `rgbaLen` bytes.
     */
    convertToRgba(): number;
    /**
     * Duration in seconds: the container's, or an elementary stream's
     * frame count over its VUI frame rate (0 if unknown).
     */
    durationHint(): number;
    /**
     * Push mode: nothing more is coming.
     */
    endOfStream(): void;
    /**
     * Nothing left to decode or show.
     */
    finished(): boolean;
    /**
     * Drop everything buffered, reset the decoder core; elementary streams
     * rewind.
     */
    flush(): void;
    /**
     * 8 to 16 (the wider of luma and chroma).
     */
    frameBitDepth(): number;
    /**
     * 1 or 2.
     */
    frameBytesPerSample(): number;
    /**
     * Frame count: an elementary stream's (counted), or the container's
     * (0 = unknown).
     */
    frameCountHint(): number;
    frameFullRange(): boolean;
    frameHeight(): number;
    /**
     * 0 = I400, 1 = I420, 2 = I422, 3 = I444.
     */
    frameLayout(): number;
    frameLen(): number;
    /**
     * ISO 23091-2 matrix code (1 BT.709, 5/6 BT.601, 9 BT.2020, 2 unspecified).
     */
    frameMatrix(): number;
    framePrimaries(): number;
    /**
     * Pointer to the packed planes of the current frame in wasm memory.
     */
    framePtr(): number;
    /**
     * pts of the access unit the picture came in (see `Decoder`); NaN if
     * it had none.
     */
    framePts(): number;
    /**
     * Frame rate as the container (or the stream's VUI) suggests; 0 if
     * unknown.
     */
    frameRateHint(): number;
    frameTransfer(): number;
    frameWidth(): number;
    framesBuffered(): number;
    hasFrame(): boolean;
    height(): number;
    /**
     * pts of the first sample of the segment most recently pushed, or NaN.
     */
    lastSegmentFirstPts(): number;
    /**
     * pts of the last sample of the segment most recently pushed, or NaN.
     */
    lastSegmentLastPts(): number;
    /**
     * pts of the last access unit handed to the decoder, or NaN.
     */
    lastSentPts(): number;
    /**
     * `maxBuffered` frames kept ahead (default 10, upstream's
     * `NUM_FRAMES_BUFFERED`); `threads` decoder threads (default 1; more
     * only on a build where `threadsSupported()` is true, else forced to 1 —
     * see `threads()` for what was applied); `codec` (`"h264"`, `"hevc"`)
     * for pushed access units, which otherwise say themselves.
     */
    constructor(max_buffered?: number | null, threads?: number | null, codec?: string | null);
    /**
     * Make the oldest buffered frame current; false if none is buffered.
     */
    nextFrame(): boolean;
    /**
     * pts of the next queued access unit (push mode), or NaN.
     */
    nextQueuedPts(): number;
    /**
     * pts of the oldest buffered frame (the one `nextFrame` would return),
     * without popping it; NaN if none is buffered or it has no pts.
     */
    peekPts(): number;
    /**
     * Access units queued and not yet decoded (push mode).
     */
    pendingInput(): number;
    planeHeight(i: number): number;
    /**
     * Byte offset of plane `i` (0 Y, 1 U, 2 V) within the frame buffer.
     */
    planeOffset(i: number): number;
    planeStride(i: number): number;
    planeWidth(i: number): number;
    /**
     * Queue one access unit (the Annex-B NAL units of one picture, start
     * codes and in-band parameter sets included) with its pts.
     */
    pushAccessUnit(data: Uint8Array | number[], pts: number): void;
    /**
     * Demux one media segment (`moof`+`mdat`, or a whole file) with rivet
     * and queue its samples as access units; returns how many. Frames keep
     * flowing across segments — no reset. Time base = the container's.
     */
    pushSegment(segment: Uint8Array | number[]): number;
    /**
     * Bytes of RGBA the *current frame* needs (`width * height * 4`).
     */
    rgbaLen(): number;
    rgbaPtr(): number;
    /**
     * `run()` until the ring is full or input runs out.
     */
    runUntilFull(): RunResult;
    /**
     * One bounded step of decoding — see `Decoder::run`. A rejected access
     * unit throws; the decoder stays usable, keep calling.
     */
    run(): RunResult;
    /**
     * Say which codec pushed access units are (`"h264"`/`"avc"`,
     * `"hevc"`/`"h265"`). A change resets the decoder.
     */
    setCodec(codec: string): void;
    /**
     * Segment-fed playback: keep the CMAF/fMP4 initialisation segment
     * (`ftyp`+`moov`) so `pushSegment` can demux media segments against it.
     * Switches to push mode. Throws without the `container` feature.
     */
    setInitSegment(init: Uint8Array | number[]): void;
    /**
     * Load a whole Annex-B elementary stream (`.264`, `.265`; copied into
     * wasm memory). `codec` is `"h264"` or `"hevc"`, or absent/`"auto"` to
     * tell from the NAL units. Resets the decoder.
     */
    setSourceAnnexB(data: Uint8Array | number[], codec?: string | null): void;
    /**
     * Load a whole container file — MP4 / fragmented MP4 / CMAF, Matroska,
     * MPEG-TS — demuxed by rivet (copied into wasm memory). Resets the
     * decoder. Throws if the build lacks the `container` feature, rivet
     * cannot read the file, or the video track is neither H.264 nor H.265.
     */
    setSourceContainer(data: Uint8Array | number[]): void;
    stats(): DecoderStats;
    /**
     * Worker threads this decoder runs with (1 unless the build supports
     * threads and more were asked for).
     */
    threads(): number;
    timeBaseDen(): number;
    /**
     * Time base numerator: `seconds = pts * num / den` for frame pts. Seconds
     * per frame from an elementary stream's VUI, or `1 / container
     * timescale`; 0 when unknown (push mode: you know your own units).
     */
    timeBaseNum(): number;
    /**
     * Push mode: drop the queued, not-yet-decoded access units with
     * `pts >= pts` (a rendition switch replaces the future from a segment
     * boundary the decoder has not reached — no flush, no gap). Returns how
     * many were dropped. See `Decoder::truncate_queued_from`.
     */
    truncateQueuedFrom(pts: number): number;
    /**
     * Access units taken by the decoder whose frames have not come out yet
     * (several pictures are decoded at once on the threads builds; 0 with
     * one thread): while this is not 0, `run()` again later collects those
     * that are done.
     */
    unitsInFlight(): number;
    /**
     * Stream width: the container header's, else the last decoded frame's; 0 if unknown yet.
     */
    width(): number;
}

/**
 * [`RunOutcome`] as a small integer for JS.
 */
export enum RunResult {
    Full = 0,
    Consumed = 1,
    Starved = 2,
    EndOfStream = 3,
}

/**
 * Entry point of a thread Worker: run the function whose address
 * `spawn_worker` gave JS. Called exactly once per address, from
 * `pkg/thread-worker.js`.
 */
export function __wasm_h26x_thread_entry(ptr: number): void;

/**
 * True when this .wasm can demux containers (`setSourceContainer`), i.e.
 * was built with the `container` feature (rivet-container).
 */
export function containerSupport(): boolean;

/**
 * True when this .wasm was built with SIMD128.
 */
export function simdEnabled(): boolean;

/**
 * Worker threads (Web Workers) the decoder threads have asked for so far in
 * this instance; 0 in a build without threads.
 */
export function threadWorkersSpawned(): number;

/**
 * True when this .wasm was built with atomics + shared memory and can run
 * the decoder's worker threads as Web Workers (`threads` > 1 in the constructor).
 * Needs a cross-origin-isolated page and the decoder in a Worker.
 */
export function threadsSupported(): boolean;

/**
 * Crate version.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly __wasm_h26x_thread_entry: (a: number) => void;
    readonly __wbg_decoderstats_free: (a: number, b: number) => void;
    readonly __wbg_get_decoderstats_accessUnitsIn: (a: number) => number;
    readonly __wbg_get_decoderstats_bytesIn: (a: number) => number;
    readonly __wbg_get_decoderstats_decodeErrors: (a: number) => number;
    readonly __wbg_get_decoderstats_framesOut: (a: number) => number;
    readonly __wbg_h26xdecoder_free: (a: number, b: number) => void;
    readonly __wbg_set_decoderstats_accessUnitsIn: (a: number, b: number) => void;
    readonly __wbg_set_decoderstats_bytesIn: (a: number, b: number) => void;
    readonly __wbg_set_decoderstats_decodeErrors: (a: number, b: number) => void;
    readonly __wbg_set_decoderstats_framesOut: (a: number, b: number) => void;
    readonly containerSupport: () => number;
    readonly h26xdecoder_codec: (a: number) => [number, number];
    readonly h26xdecoder_convertToRgba: (a: number) => [number, number, number];
    readonly h26xdecoder_durationHint: (a: number) => number;
    readonly h26xdecoder_endOfStream: (a: number) => void;
    readonly h26xdecoder_finished: (a: number) => number;
    readonly h26xdecoder_flush: (a: number) => [number, number];
    readonly h26xdecoder_frameBitDepth: (a: number) => number;
    readonly h26xdecoder_frameBytesPerSample: (a: number) => number;
    readonly h26xdecoder_frameCountHint: (a: number) => number;
    readonly h26xdecoder_frameFullRange: (a: number) => number;
    readonly h26xdecoder_frameHeight: (a: number) => number;
    readonly h26xdecoder_frameLayout: (a: number) => number;
    readonly h26xdecoder_frameLen: (a: number) => number;
    readonly h26xdecoder_frameMatrix: (a: number) => number;
    readonly h26xdecoder_framePrimaries: (a: number) => number;
    readonly h26xdecoder_framePtr: (a: number) => number;
    readonly h26xdecoder_framePts: (a: number) => number;
    readonly h26xdecoder_frameRateHint: (a: number) => number;
    readonly h26xdecoder_frameTransfer: (a: number) => number;
    readonly h26xdecoder_frameWidth: (a: number) => number;
    readonly h26xdecoder_framesBuffered: (a: number) => number;
    readonly h26xdecoder_hasFrame: (a: number) => number;
    readonly h26xdecoder_height: (a: number) => number;
    readonly h26xdecoder_lastSegmentFirstPts: (a: number) => number;
    readonly h26xdecoder_lastSegmentLastPts: (a: number) => number;
    readonly h26xdecoder_lastSentPts: (a: number) => number;
    readonly h26xdecoder_new: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly h26xdecoder_nextFrame: (a: number) => number;
    readonly h26xdecoder_nextQueuedPts: (a: number) => number;
    readonly h26xdecoder_peekPts: (a: number) => number;
    readonly h26xdecoder_pendingInput: (a: number) => number;
    readonly h26xdecoder_planeHeight: (a: number, b: number) => number;
    readonly h26xdecoder_planeOffset: (a: number, b: number) => number;
    readonly h26xdecoder_planeStride: (a: number, b: number) => number;
    readonly h26xdecoder_planeWidth: (a: number, b: number) => number;
    readonly h26xdecoder_pushAccessUnit: (a: number, b: number, c: number, d: number) => [number, number];
    readonly h26xdecoder_pushSegment: (a: number, b: number, c: number) => [number, number, number];
    readonly h26xdecoder_rgbaLen: (a: number) => number;
    readonly h26xdecoder_rgbaPtr: (a: number) => number;
    readonly h26xdecoder_run: (a: number) => [number, number, number];
    readonly h26xdecoder_runUntilFull: (a: number) => [number, number, number];
    readonly h26xdecoder_setCodec: (a: number, b: number, c: number) => [number, number];
    readonly h26xdecoder_setInitSegment: (a: number, b: number, c: number) => [number, number];
    readonly h26xdecoder_setSourceAnnexB: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly h26xdecoder_setSourceContainer: (a: number, b: number, c: number) => [number, number];
    readonly h26xdecoder_stats: (a: number) => number;
    readonly h26xdecoder_threads: (a: number) => number;
    readonly h26xdecoder_timeBaseDen: (a: number) => number;
    readonly h26xdecoder_timeBaseNum: (a: number) => number;
    readonly h26xdecoder_truncateQueuedFrom: (a: number, b: number) => number;
    readonly h26xdecoder_unitsInFlight: (a: number) => number;
    readonly h26xdecoder_width: (a: number) => number;
    readonly simdEnabled: () => number;
    readonly threadWorkersSpawned: () => number;
    readonly threadsSupported: () => number;
    readonly version: () => [number, number];
    readonly memory: WebAssembly.Memory;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_thread_destroy: (a?: number, b?: number, c?: number) => void;
    readonly __wbindgen_start: (a: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput, memory?: WebAssembly.Memory, thread_stack_size?: number }} module - Passing `SyncInitInput` directly is deprecated.
 * @param {WebAssembly.Memory} memory - Deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput, memory?: WebAssembly.Memory, thread_stack_size?: number } | SyncInitInput, memory?: WebAssembly.Memory): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput>, memory?: WebAssembly.Memory, thread_stack_size?: number }} module_or_path - Passing `InitInput` directly is deprecated.
 * @param {WebAssembly.Memory} memory - Deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput>, memory?: WebAssembly.Memory, thread_stack_size?: number } | InitInput | Promise<InitInput>, memory?: WebAssembly.Memory): Promise<InitOutput>;
