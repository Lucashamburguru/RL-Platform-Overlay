#!/usr/bin/env bash
# Capture the real UI with sample data and build the README animation.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
command -v magick >/dev/null || { echo "Install ImageMagick 7 first." >&2; exit 1; }
tour_dir="$(mktemp -d /tmp/rl-page-tour.XXXXXX)"
RL_PAGE_TOUR_DIR="$tour_dir" cargo test --locked --lib capture_readme_pages -- --ignored

pages=(01-setup 02-lobby 03-session 04-boost 05-dashboard 06-item-swapper
       07-engine-audio 08-uploader 09-replay-library 09b-replay-details
       10-replay-tools 11-history 11b-history-record 12-support 13-debug)
titles=("Setup" "Overlay / Lobby" "Overlay / Session" "Overlay / Boost"
        "Dashboard settings" "Item Swapper" "Engine Audio" "Replay uploader"
        "Replay Library" "Replay details" "Replay tools" "Player History"
        "Player records" "Support" "Debug (optional)")
frames=()
for index in "${!pages[@]}"; do
    frame="$tour_dir/frame-${pages[$index]}.png"
    magick -size 1000x42 xc:'#1b1b1b' -font DejaVu-Sans -pointsize 18 \
        -fill '#78d5ec' -gravity West -annotate +16+0 "${titles[$index]}" \
        -font DejaVu-Sans -pointsize 12 -fill '#bbbbbb' -gravity East \
        -annotate +16+0 "Sample data · $((index + 1)) / ${#pages[@]}" \
        "$tour_dir/${pages[$index]}.png" -append +repage "$frame"
    frames+=("$frame")
done
magick -delay 300 "${frames[@]}" -loop 0 -colors 128 -layers Optimize assets/page-tour.gif
echo "Created assets/page-tour.gif. Source captures: $tour_dir"
