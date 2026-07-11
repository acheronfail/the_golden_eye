#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
output="$root/test/clips/replay-buffer-60s.mp4"

# The large seven-segment decimal counter is for humans. The six boxes along the bottom are
# a machine-readable little-endian binary counter (white = 1, black = 0).  It
# is deliberately burned into the video rather than stored as container
# metadata: trimming/remuxing can rebase PTS and discard arbitrary metadata,
# while these pixels survive the exact operation the plugin performs.
# Draw one seven-segment digit. Segment expressions are based on the decimal
# digit at floor(t); this avoids requiring ffmpeg's optional drawtext filter.
filters="null"
add_digit() {
  local x="$1" divisor="$2" digit="mod(floor(t/${2}),10)"
  filters+=",drawbox=x=${x}:y=75:w=92:h=16:color=white:t=fill:enable='not(eq(${digit},1)+eq(${digit},4))'"
  filters+=",drawbox=x=$((x + 76)):y=83:w=16:h=78:color=white:t=fill:enable='not(eq(${digit},5)+eq(${digit},6))'"
  filters+=",drawbox=x=$((x + 76)):y=169:w=16:h=78:color=white:t=fill:enable='not(eq(${digit},2))'"
  filters+=",drawbox=x=${x}:y=239:w=92:h=16:color=white:t=fill:enable='not(eq(${digit},1)+eq(${digit},4)+eq(${digit},7))'"
  filters+=",drawbox=x=${x}:y=169:w=16:h=78:color=white:t=fill:enable='eq(${digit},0)+eq(${digit},2)+eq(${digit},6)+eq(${digit},8)'"
  filters+=",drawbox=x=${x}:y=83:w=16:h=78:color=white:t=fill:enable='not(eq(${digit},1)+eq(${digit},2)+eq(${digit},3)+eq(${digit},7))'"
  filters+=",drawbox=x=${x}:y=157:w=92:h=16:color=white:t=fill:enable='not(eq(${digit},0)+eq(${digit},1)+eq(${digit},7))'"
}
add_digit 210 10
add_digit 338 1

for bit in 0 1 2 3 4 5; do
  x=$((104 + bit * 72))
  divisor=$((1 << bit))
  filters+=",drawbox=x=${x}:y=310:w=56:h=30:color=black:t=fill"
  filters+=",drawbox=x=${x}:y=310:w=56:h=30:color=white:t=fill:enable='mod(floor(t/${divisor}),2)'"
done

ffmpeg -hide_banner -loglevel error -y \
  -f lavfi -i "color=c=gray:s=640x360:r=10:d=60" \
  -vf "$filters" \
  -c:v libx264 -pix_fmt yuv420p -preset veryfast \
  -g 10 -keyint_min 10 -sc_threshold 0 -movflags +faststart \
  -metadata comment="Golden Eye replay fixture: decimal counter plus 6-bit visual timestamp" \
  "$output"

echo "generated $output"
