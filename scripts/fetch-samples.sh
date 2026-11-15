#!/usr/bin/env bash
# Pull real-content H.264 and H.265 clips into testdata/samples/ for the demo
# page and the benchmarks (scripts/bench.mjs, scripts/bench-browser.mjs) —
# not committed (the "pull from upstream rather than vendor" rule applied to
# test data).
#
# Big Buck Bunny (CC-BY) from test-videos.co.uk, 10 s at 30 fps, as MP4; the
# elementary stream next to each is ffmpeg's `-c copy` with the parameter
# sets put in band (no decode, no re-encode). With H26XDEC set (rivet-h26x's
# h26xdec, see scripts/make-fixtures.sh) each gets a .md5 of its frames
# beside it, which bench.mjs checks the wasm against.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p testdata/samples
base=https://test-videos.co.uk/vids/bigbuckbunny/mp4
clips=(
  "h264-360p   h264  h264/360/Big_Buck_Bunny_360_10s_5MB.mp4"
  "h264-720p   h264  h264/720/Big_Buck_Bunny_720_10s_5MB.mp4"
  "h264-1080p  h264  h264/1080/Big_Buck_Bunny_1080_10s_5MB.mp4"
  "hevc-720p   hevc  h265/720/Big_Buck_Bunny_720_10s_5MB.mp4"
  "hevc-1080p  hevc  h265/1080/Big_Buck_Bunny_1080_10s_5MB.mp4"
)
for spec in "${clips[@]}"; do
  read -r name codec rel <<<"$spec"
  mp4="testdata/samples/$name.mp4"
  ext=$([ "$codec" = hevc ] && echo 265 || echo 264)
  es="testdata/samples/$name.$ext"
  if [ ! -s "$mp4" ]; then
    echo "fetching $name"
    curl -fsSL -A "wasm-h26x-samples/1.0" "$base/$rel" -o "$mp4.part" && mv "$mp4.part" "$mp4"
  fi
  if [ ! -s "$es" ]; then
    ffmpeg -hide_banner -loglevel error -y -i "$mp4" -an -c:v copy -bsf:v "${codec}_mp4toannexb" -f "$codec" "$es"
  fi
  if [ -n "${H26XDEC:-}" ] && [ ! -s "$es.md5" ]; then
    H26XDEC_NO_CHROMA_PAD=1 "$H26XDEC" "$es" | cut -d, -f5 > "$es.md5"
  fi
  printf '%-12s %s%s\n' "$name" "$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,profile,width,height,r_frame_rate,nb_frames -of csv=p=0 "$mp4")" \
    "$([ -s "$es.md5" ] && echo "  $(wc -l < "$es.md5" | tr -d ' ') frame MD5s" || true)"
done
