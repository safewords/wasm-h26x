/* @ts-self-types="./wasm_h26x.d.ts" */

/**
 * Decoder counters.
 */
export class DecoderStats {
    static __wrap(ptr) {
        const obj = Object.create(DecoderStats.prototype);
        obj.__wbg_ptr = ptr;
        DecoderStatsFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        DecoderStatsFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_decoderstats_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    get accessUnitsIn() {
        const ret = wasm.__wbg_get_decoderstats_accessUnitsIn(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    get bytesIn() {
        const ret = wasm.__wbg_get_decoderstats_bytesIn(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    get decodeErrors() {
        const ret = wasm.__wbg_get_decoderstats_decodeErrors(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    get framesOut() {
        const ret = wasm.__wbg_get_decoderstats_framesOut(this.__wbg_ptr);
        return ret;
    }
    /**
     * @param {number} arg0
     */
    set accessUnitsIn(arg0) {
        wasm.__wbg_set_decoderstats_accessUnitsIn(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set bytesIn(arg0) {
        wasm.__wbg_set_decoderstats_bytesIn(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set decodeErrors(arg0) {
        wasm.__wbg_set_decoderstats_decodeErrors(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set framesOut(arg0) {
        wasm.__wbg_set_decoderstats_framesOut(this.__wbg_ptr, arg0);
    }
}
if (Symbol.dispose) DecoderStats.prototype[Symbol.dispose] = DecoderStats.prototype.free;

/**
 * One H.264 / H.265 decoder with its frame ring and a reusable RGBA
 * scratch buffer.
 */
export class H26xDecoder {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        H26xDecoderFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_h26xdecoder_free(ptr, 0);
    }
    /**
     * `"h264"`, `"hevc"`, or `""` while unknown.
     * @returns {string}
     */
    codec() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.h26xdecoder_codec(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Convert the current frame to RGBA8 into the internal scratch buffer
     * (SIMD128 in the SIMD build) and return its pointer; `rgbaLen` bytes.
     * @returns {number}
     */
    convertToRgba() {
        const ret = wasm.h26xdecoder_convertToRgba(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * Duration in seconds: the container's, or an elementary stream's
     * frame count over its VUI frame rate (0 if unknown).
     * @returns {number}
     */
    durationHint() {
        const ret = wasm.h26xdecoder_durationHint(this.__wbg_ptr);
        return ret;
    }
    /**
     * Push mode: nothing more is coming.
     */
    endOfStream() {
        wasm.h26xdecoder_endOfStream(this.__wbg_ptr);
    }
    /**
     * Nothing left to decode or show.
     * @returns {boolean}
     */
    finished() {
        const ret = wasm.h26xdecoder_finished(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Drop everything buffered, reset the decoder core; elementary streams
     * rewind.
     */
    flush() {
        const ret = wasm.h26xdecoder_flush(this.__wbg_ptr);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * 8 to 16 (the wider of luma and chroma).
     * @returns {number}
     */
    frameBitDepth() {
        const ret = wasm.h26xdecoder_frameBitDepth(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * 1 or 2.
     * @returns {number}
     */
    frameBytesPerSample() {
        const ret = wasm.h26xdecoder_frameBytesPerSample(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Frame count: an elementary stream's (counted), or the container's
     * (0 = unknown).
     * @returns {number}
     */
    frameCountHint() {
        const ret = wasm.h26xdecoder_frameCountHint(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {boolean}
     */
    frameFullRange() {
        const ret = wasm.h26xdecoder_frameFullRange(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {number}
     */
    frameHeight() {
        const ret = wasm.h26xdecoder_frameHeight(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * 0 = I400, 1 = I420, 2 = I422, 3 = I444.
     * @returns {number}
     */
    frameLayout() {
        const ret = wasm.h26xdecoder_frameLayout(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    frameLen() {
        const ret = wasm.h26xdecoder_frameLen(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * ISO 23091-2 matrix code (1 BT.709, 5/6 BT.601, 9 BT.2020, 2 unspecified).
     * @returns {number}
     */
    frameMatrix() {
        const ret = wasm.h26xdecoder_frameMatrix(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    framePrimaries() {
        const ret = wasm.h26xdecoder_framePrimaries(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Pointer to the packed planes of the current frame in wasm memory.
     * @returns {number}
     */
    framePtr() {
        const ret = wasm.h26xdecoder_framePtr(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * pts of the access unit the picture came in (see `Decoder`); NaN if
     * it had none.
     * @returns {number}
     */
    framePts() {
        const ret = wasm.h26xdecoder_framePts(this.__wbg_ptr);
        return ret;
    }
    /**
     * Frame rate as the container (or the stream's VUI) suggests; 0 if
     * unknown.
     * @returns {number}
     */
    frameRateHint() {
        const ret = wasm.h26xdecoder_frameRateHint(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    frameTransfer() {
        const ret = wasm.h26xdecoder_frameTransfer(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    frameWidth() {
        const ret = wasm.h26xdecoder_frameWidth(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    framesBuffered() {
        const ret = wasm.h26xdecoder_framesBuffered(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {boolean}
     */
    hasFrame() {
        const ret = wasm.h26xdecoder_hasFrame(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {number}
     */
    height() {
        const ret = wasm.h26xdecoder_height(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * pts of the first sample of the segment most recently pushed, or NaN.
     * @returns {number}
     */
    lastSegmentFirstPts() {
        const ret = wasm.h26xdecoder_lastSegmentFirstPts(this.__wbg_ptr);
        return ret;
    }
    /**
     * pts of the last sample of the segment most recently pushed, or NaN.
     * @returns {number}
     */
    lastSegmentLastPts() {
        const ret = wasm.h26xdecoder_lastSegmentLastPts(this.__wbg_ptr);
        return ret;
    }
    /**
     * pts of the last access unit handed to the decoder, or NaN.
     * @returns {number}
     */
    lastSentPts() {
        const ret = wasm.h26xdecoder_lastSentPts(this.__wbg_ptr);
        return ret;
    }
    /**
     * `maxBuffered` frames kept ahead (default 10, upstream's
     * `NUM_FRAMES_BUFFERED`); `threads` decoder threads (default 1; more
     * only on a build where `threadsSupported()` is true, else forced to 1 —
     * see `threads()` for what was applied); `codec` (`"h264"`, `"hevc"`)
     * for pushed access units, which otherwise say themselves.
     * @param {number | null} [max_buffered]
     * @param {number | null} [threads]
     * @param {string | null} [codec]
     */
    constructor(max_buffered, threads, codec) {
        const val2 = +(isLikeNone(max_buffered) ? Number.MAX_SAFE_INTEGER : (max_buffered) >>> 0);
        const val3 = +(isLikeNone(threads) ? Number.MAX_SAFE_INTEGER : (threads) >>> 0);
        var ptr0 = isLikeNone(codec) ? 0 : passStringToWasm0(codec, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len0 = WASM_VECTOR_LEN;
        const ret = wasm.h26xdecoder_new(val2, val3, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        H26xDecoderFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Make the oldest buffered frame current; false if none is buffered.
     * @returns {boolean}
     */
    nextFrame() {
        const ret = wasm.h26xdecoder_nextFrame(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * pts of the next queued access unit (push mode), or NaN.
     * @returns {number}
     */
    nextQueuedPts() {
        const ret = wasm.h26xdecoder_nextQueuedPts(this.__wbg_ptr);
        return ret;
    }
    /**
     * pts of the oldest buffered frame (the one `nextFrame` would return),
     * without popping it; NaN if none is buffered or it has no pts.
     * @returns {number}
     */
    peekPts() {
        const ret = wasm.h26xdecoder_peekPts(this.__wbg_ptr);
        return ret;
    }
    /**
     * Access units queued and not yet decoded (push mode).
     * @returns {number}
     */
    pendingInput() {
        const ret = wasm.h26xdecoder_pendingInput(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @param {number} i
     * @returns {number}
     */
    planeHeight(i) {
        const ret = wasm.h26xdecoder_planeHeight(this.__wbg_ptr, i);
        return ret >>> 0;
    }
    /**
     * Byte offset of plane `i` (0 Y, 1 U, 2 V) within the frame buffer.
     * @param {number} i
     * @returns {number}
     */
    planeOffset(i) {
        const ret = wasm.h26xdecoder_planeOffset(this.__wbg_ptr, i);
        return ret >>> 0;
    }
    /**
     * @param {number} i
     * @returns {number}
     */
    planeStride(i) {
        const ret = wasm.h26xdecoder_planeStride(this.__wbg_ptr, i);
        return ret >>> 0;
    }
    /**
     * @param {number} i
     * @returns {number}
     */
    planeWidth(i) {
        const ret = wasm.h26xdecoder_planeWidth(this.__wbg_ptr, i);
        return ret >>> 0;
    }
    /**
     * Queue one access unit (the Annex-B NAL units of one picture, start
     * codes and in-band parameter sets included) with its pts.
     * @param {Uint8Array | number[]} data
     * @param {number} pts
     */
    pushAccessUnit(data, pts) {
        const val2 = +pts;
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.h26xdecoder_pushAccessUnit(this.__wbg_ptr, ptr0, len0, val2);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Demux one media segment (`moof`+`mdat`, or a whole file) with rivet
     * and queue its samples as access units; returns how many. Frames keep
     * flowing across segments — no reset. Time base = the container's.
     * @param {Uint8Array | number[]} segment
     * @returns {number}
     */
    pushSegment(segment) {
        const ptr0 = passArray8ToWasm0(segment, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.h26xdecoder_pushSegment(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * Bytes of RGBA the *current frame* needs (`width * height * 4`).
     * @returns {number}
     */
    rgbaLen() {
        const ret = wasm.h26xdecoder_rgbaLen(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    rgbaPtr() {
        const ret = wasm.h26xdecoder_rgbaPtr(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * `run()` until the ring is full or input runs out.
     * @returns {RunResult}
     */
    runUntilFull() {
        const ret = wasm.h26xdecoder_runUntilFull(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0];
    }
    /**
     * One bounded step of decoding — see `Decoder::run`. A rejected access
     * unit throws; the decoder stays usable, keep calling.
     * @returns {RunResult}
     */
    run() {
        const ret = wasm.h26xdecoder_run(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0];
    }
    /**
     * Say which codec pushed access units are (`"h264"`/`"avc"`,
     * `"hevc"`/`"h265"`). A change resets the decoder.
     * @param {string} codec
     */
    setCodec(codec) {
        const ptr0 = passStringToWasm0(codec, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.h26xdecoder_setCodec(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Segment-fed playback: keep the CMAF/fMP4 initialisation segment
     * (`ftyp`+`moov`) so `pushSegment` can demux media segments against it.
     * Switches to push mode. Throws without the `container` feature.
     * @param {Uint8Array | number[]} init
     */
    setInitSegment(init) {
        const ptr0 = passArray8ToWasm0(init, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.h26xdecoder_setInitSegment(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Load a whole Annex-B elementary stream (`.264`, `.265`; copied into
     * wasm memory). `codec` is `"h264"` or `"hevc"`, or absent/`"auto"` to
     * tell from the NAL units. Resets the decoder.
     * @param {Uint8Array | number[]} data
     * @param {string | null} [codec]
     */
    setSourceAnnexB(data, codec) {
        let ptr0, len0, ptr1, len1;
        try {
            ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
            len0 = WASM_VECTOR_LEN;
            ptr1 = isLikeNone(codec) ? 0 : passStringToWasm0(codec, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            len1 = WASM_VECTOR_LEN;
        } catch (e) {
            if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 1, 1); }
            if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 1, 1); }
            throw e;
        }
        const ret = wasm.h26xdecoder_setSourceAnnexB(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Load a whole container file — MP4 / fragmented MP4 / CMAF, Matroska,
     * MPEG-TS — demuxed by rivet (copied into wasm memory). Resets the
     * decoder. Throws if the build lacks the `container` feature, rivet
     * cannot read the file, or the video track is neither H.264 nor H.265.
     * @param {Uint8Array | number[]} data
     */
    setSourceContainer(data) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.h26xdecoder_setSourceContainer(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @returns {DecoderStats}
     */
    stats() {
        const ret = wasm.h26xdecoder_stats(this.__wbg_ptr);
        return DecoderStats.__wrap(ret);
    }
    /**
     * Worker threads this decoder runs with (1 unless the build supports
     * threads and more were asked for).
     * @returns {number}
     */
    threads() {
        const ret = wasm.h26xdecoder_threads(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    timeBaseDen() {
        const ret = wasm.h26xdecoder_timeBaseDen(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Time base numerator: `seconds = pts * num / den` for frame pts. Seconds
     * per frame from an elementary stream's VUI, or `1 / container
     * timescale`; 0 when unknown (push mode: you know your own units).
     * @returns {number}
     */
    timeBaseNum() {
        const ret = wasm.h26xdecoder_timeBaseNum(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Push mode: drop the queued, not-yet-decoded access units with
     * `pts >= pts` (a rendition switch replaces the future from a segment
     * boundary the decoder has not reached — no flush, no gap). Returns how
     * many were dropped. See `Decoder::truncate_queued_from`.
     * @param {number} pts
     * @returns {number}
     */
    truncateQueuedFrom(pts) {
        const ret = wasm.h26xdecoder_truncateQueuedFrom(this.__wbg_ptr, pts);
        return ret >>> 0;
    }
    /**
     * Access units taken by the decoder whose frames have not come out yet
     * (several pictures are decoded at once on the threads builds; 0 with
     * one thread): while this is not 0, `run()` again later collects those
     * that are done.
     * @returns {number}
     */
    unitsInFlight() {
        const ret = wasm.h26xdecoder_unitsInFlight(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Stream width: the container header's, else the last decoded frame's; 0 if unknown yet.
     * @returns {number}
     */
    width() {
        const ret = wasm.h26xdecoder_width(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) H26xDecoder.prototype[Symbol.dispose] = H26xDecoder.prototype.free;

/**
 * [`RunOutcome`] as a small integer for JS.
 * @enum {0 | 1 | 2 | 3}
 */
export const RunResult = Object.freeze({
    Full: 0, "0": "Full",
    Consumed: 1, "1": "Consumed",
    Starved: 2, "2": "Starved",
    EndOfStream: 3, "3": "EndOfStream",
});

/**
 * True when this .wasm can demux containers (`setSourceContainer`), i.e.
 * was built with the `container` feature (rivet-container).
 * @returns {boolean}
 */
export function containerSupport() {
    const ret = wasm.containerSupport();
    return ret !== 0;
}

/**
 * True when this .wasm was built with SIMD128.
 * @returns {boolean}
 */
export function simdEnabled() {
    const ret = wasm.simdEnabled();
    return ret !== 0;
}

/**
 * Worker threads (Web Workers) the decoder threads have asked for so far in
 * this instance; 0 in a build without threads.
 * @returns {number}
 */
export function threadWorkersSpawned() {
    const ret = wasm.threadWorkersSpawned();
    return ret >>> 0;
}

/**
 * True when this .wasm was built with atomics + shared memory and can run
 * the decoder's worker threads as Web Workers (`threads` > 1 in the constructor).
 * Needs a cross-origin-isolated page and the decoder in a Worker.
 * @returns {boolean}
 */
export function threadsSupported() {
    const ret = wasm.threadsSupported();
    return ret !== 0;
}

/**
 * Crate version.
 * @returns {string}
 */
export function version() {
    let deferred1_0;
    let deferred1_1;
    try {
        const ret = wasm.version();
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_208446807d0d1e30: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg___wbindgen_throw_4c88d8bf264fd984: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            table.set(0, undefined);
            table.set(1024, undefined);
            table.set(1025, null);
            table.set(1026, true);
            table.set(1027, false);
        },
    };
    return {
        __proto__: null,
        "./wasm_h26x_bg.js": import0,
    };
}

const DecoderStatsFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_decoderstats_free(ptr, 1));
const H26xDecoderFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_h26xdecoder_free(ptr, 1));

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(bytes) {
    numBytesDecoded += bytes.length;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
        cachedTextDecoder.decode();
        numBytesDecoded = bytes.length;
    }
    return cachedTextDecoder.decode(bytes);
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

let __wbg_init_promise;
async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;
    if (__wbg_init_promise === undefined) {
        __wbg_init_promise = __wbg_init_once(module_or_path);
        __wbg_init_promise.catch(() => { __wbg_init_promise = undefined; });
    }
    return __wbg_init_promise;
}

async function __wbg_init_once(module_or_path) {

    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('wasm_h26x_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
