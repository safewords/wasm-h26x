#!/usr/bin/env bash
# Regenerate testdata/: small H.264 and H.265 streams, the same streams in
# containers, and the per-frame MD5 of their decoded output as rivet-h26x's
# own native decoder (its `h26xdec` example) gives it. The Rust tests and the
# Node tests decode each fixture and must reproduce every frame's MD5 — the
# decoding process of both standards is normative, and rivet-h26x is
# bit-exact against the JVT and JCT-VC conformance suites natively, so any
# difference is a bug in the wasm build (or in this crate).
#
# The encoders are run as programs (ffmpeg with libx264 / libx265); nothing
# here reads or links another decoder. Needs:
#   - an ffmpeg with libx264 and libx265
#   - H26XDEC: rivet-h26x's h26xdec, built from the revision Cargo.lock pins:
#       git clone https://github.com/safewords/rivet-h26x-codecs && cd rivet-h26x-codecs
#       git checkout <rev from Cargo.lock> && cargo build --release --example h26xdec
#       H26XDEC=$PWD/target/release/examples/h26xdec scripts/make-fixtures.sh
#
# Each fixture is `<name>.264|.265` (Annex-B) plus `<name>.md5`: h26xdec's
# output verbatim, one line per output frame (`index,poc,decode_index,WxH,md5`
# of the packed planar picture, Y then Cb then Cr, little-endian 16-bit
# samples above 8 bits).
set -euo pipefail
cd "$(dirname "$0")/.."
# A 4:0:0 frame is its luma plane alone, here as in wasm-av1 (layout I400);
# h26xdec pads H.264 4:0:0 with grey chroma planes to match JM's files
# unless told not to.
export H26XDEC_NO_CHROMA_PAD=1
: "${H26XDEC:?set H26XDEC to the h26xdec example of rivet-h26x, see the header of this script}"
mkdir -p testdata
rm -f testdata/*.264 testdata/*.265 testdata/*.md5 testdata/*.mp4 testdata/*.fmp4 testdata/*.mkv testdata/*.ts

ff() { ffmpeg -hide_banner -loglevel error -y "$@"; }

# name   codec  size    frames  pix_fmt       encoder arguments
#
# testsrc2 is a moving pattern with text; -g 12 forces keyframes so that a
# bug in inter prediction cannot hide behind one long GOP. One encoder
# thread keeps the streams the same on every machine.
FIXTURES=(
  "h264-cavlc-320x180      h264 320x180 48 yuv420p     -profile:v baseline -coder 0 -bf 0"
  "h264-cabac-b-320x180    h264 320x180 48 yuv420p     -profile:v high -bf 3 -b_strategy 2 -x264-params b-pyramid=normal:weightb=1:8x8dct=1"
  "h264-high10-160x90      h264 160x90  24 yuv420p10le -profile:v high10 -bf 2"
  "h264-422p10-96x64       h264 96x64   12 yuv422p10le -profile:v high422 -bf 1"
  "h264-444-96x64          h264 96x64   12 yuv444p     -profile:v high444 -bf 1"
  "h264-gray-96x64         h264 96x64   12 gray        -profile:v high -bf 1"
  "hevc-main-320x180       hevc 320x180 48 yuv420p     -x265-params bframes=3:b-pyramid=1:wpp=1:log-level=error"
  "hevc-main10-178x100     hevc 178x100 24 yuv420p10le -x265-params bframes=2:wpp=1:log-level=error"
  "hevc-444-96x64          hevc 96x64   12 yuv444p     -x265-params bframes=1:log-level=error"
)

for spec in "${FIXTURES[@]}"; do
  read -r name codec size frames pix_fmt extra <<<"$spec"
  ext=$([ "$codec" = hevc ] && echo 265 || echo 264)
  echo "== $name ($codec, $size, $frames frames, $pix_fmt)"
  if [ "$codec" = hevc ]; then
    enc=(-c:v libx265 -preset fast -crf 30 -g 12 -keyint_min 12 -threads 1)
    bsf=hevc_mp4toannexb
  else
    enc=(-c:v libx264 -preset fast -crf 30 -g 12 -keyint_min 12 -sc_threshold 0 -threads 1)
    bsf=h264_mp4toannexb
  fi
  # Encoded into MP4 first, so the stream has real timestamps; the
  # elementary stream is that MP4's samples with the parameter sets in band.
  # shellcheck disable=SC2086
  ff -f lavfi -i "testsrc2=size=${size}:rate=24" -frames:v "$frames" \
    -vf "format=${pix_fmt}" -s "$size" "${enc[@]}" $extra "testdata/$name.mp4"
  ff -i "testdata/$name.mp4" -c:v copy -bsf:v "$bsf" -f "$codec" "testdata/$name.$ext"
  "$H26XDEC" "testdata/$name.$ext" > "testdata/$name.md5" 2> testdata/h26xdec.log
  cat testdata/h26xdec.log
  got=$(grep -c , "testdata/$name.md5")
  [ "$got" = "$frames" ] || { echo "$name: h26xdec gave $got frames, not $frames" >&2; exit 1; }
  grep -q ' 0 warnings' testdata/h26xdec.log || { echo "$name: h26xdec warned" >&2; exit 1; }
  rm testdata/h26xdec.log
done

# The containers. Every one holds a stream above, remuxed (no re-encode), and
# must decode to that stream's MD5s: H.264 with B-frames (so presentation
# order differs from decode order) in a plain MP4, a fragmented MP4, Matroska
# and MPEG-TS; HEVC in MP4 (hvc1), Matroska and TS.
keep_mp4="h264-cabac-b-320x180 hevc-main-320x180"
for name in $keep_mp4; do
  src="testdata/$name.mp4"
  tag=$([ "${name%%-*}" = hevc ] && echo "-tag:v hvc1" || echo "")
  # shellcheck disable=SC2086
  ff -i "$src" -c:v copy $tag -movflags +faststart "testdata/$name.tmp.mp4" && mv "testdata/$name.tmp.mp4" "$src"
  ff -i "$src" -c:v copy -f matroska "testdata/$name.mkv"
  ff -i "$src" -c:v copy -f mpegts -mpegts_copyts 0 "testdata/$name.ts"
done
ff -i testdata/h264-cabac-b-320x180.mp4 -c:v copy -f mp4 \
  -movflags frag_keyframe+empty_moov+default_base_moof+dash testdata/h264-cabac-b-320x180.fmp4
# The MP4s the other streams were encoded into are not fixtures.
for spec in "${FIXTURES[@]}"; do
  read -r name _ <<<"$spec"
  case " $keep_mp4 " in *" $name "*) ;; *) rm -f "testdata/$name.mp4" ;; esac
done

# CMAF as HLS serves it: an init segment plus 1-second fMP4 media segments,
# for the segment-fed API (setInitSegment / pushSegment), of the H.264
# stream with B-frames.
rm -rf testdata/cmaf testdata/cmaf-lo && mkdir -p testdata/cmaf testdata/cmaf-lo
ff -i testdata/h264-cabac-b-320x180.mp4 -c:v copy -f hls -hls_segment_type fmp4 -hls_time 1 \
  -hls_playlist_type vod -hls_fmp4_init_filename init.mp4 -hls_segment_filename testdata/cmaf/seg%d.m4s testdata/cmaf/index.m3u8

# A second rendition of the same content — 160x90, the same GOP and the same
# 1 s segmentation, as a ladder rung is — for the seamless rendition switch
# (truncate the queued future at a segment boundary, new init, other rung's
# segments). Its frames are checked against its own elementary stream.
ff -f lavfi -i "testsrc2=size=160x90:rate=24" -frames:v 48 -vf format=yuv420p -s 160x90 \
  -c:v libx264 -preset fast -crf 30 -g 12 -keyint_min 12 -sc_threshold 0 -threads 1 \
  -profile:v high -bf 3 -b_strategy 2 testdata/cmaf-lo/src.mp4
ff -i testdata/cmaf-lo/src.mp4 -c:v copy -bsf:v h264_mp4toannexb -f h264 testdata/h264-cmaf-lo-160x90.264
"$H26XDEC" testdata/h264-cmaf-lo-160x90.264 > testdata/h264-cmaf-lo-160x90.md5 2>/dev/null
ff -i testdata/cmaf-lo/src.mp4 -c:v copy -f hls -hls_segment_type fmp4 -hls_time 1 \
  -hls_playlist_type vod -hls_fmp4_init_filename init.mp4 -hls_segment_filename testdata/cmaf-lo/seg%d.m4s testdata/cmaf-lo/index.m3u8
rm testdata/cmaf-lo/src.mp4

# The H.265 stream as CMAF too: x265 makes open GOPs (CRA pictures whose
# RASL pictures refer to the segment before), which a continuous decode
# must decode, not skip.
rm -rf testdata/cmaf-hevc && mkdir -p testdata/cmaf-hevc
ff -i testdata/hevc-main-320x180.mp4 -c:v copy -tag:v hvc1 -f hls -hls_segment_type fmp4 -hls_time 1 \
  -hls_playlist_type vod -hls_fmp4_init_filename init.mp4 -hls_segment_filename testdata/cmaf-hevc/seg%d.m4s testdata/cmaf-hevc/index.m3u8

# A two-rung master playlist over the two, for HlsH26xVideo's rung switching
# (BANDWIDTH from the segment sizes, roughly).
cat > testdata/cmaf/master.m3u8 <<'EOF'
#EXTM3U
#EXT-X-VERSION:7
#EXT-X-INDEPENDENT-SEGMENTS
#EXT-X-STREAM-INF:BANDWIDTH=200000,AVERAGE-BANDWIDTH=180000,RESOLUTION=160x90,CODECS="avc1.64000c",FRAME-RATE=24.000
../cmaf-lo/index.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=600000,AVERAGE-BANDWIDTH=520000,RESOLUTION=320x180,CODECS="avc1.64000d",FRAME-RATE=24.000
index.m3u8
EOF

ls -la testdata testdata/cmaf testdata/cmaf-lo testdata/cmaf-hevc
du -sh testdata
