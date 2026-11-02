#!/bin/sh
# Capture the README screenshot and demo GIF of the desktop app.
#
# Hyprland-only (tested on omarchy, 2x display). Needs: cargo-tauri, grim, wf-recorder,
# ffmpeg, magick, jq, ydotool with ydotoold running as root (sudo ydotoold &).
#
#   scripts/capture-media.sh            # writes screenshots/numr-desktop-linux.png and -demo.gif
#   NUMR_WEB_DIR=../numr-web scripts/capture-media.sh
#
# What it does: builds an opaque, square-cornered copy of the UI, runs `cargo tauri dev`
# with it, floats the window at a fixed size, strips compositor rounding/border, loads
# docs/media/screenshot.numr for the PNG, then types docs/media/demo.numr live and opens the
# settings panel while wf-recorder records. Keyboard layouts and Hyprland settings are
# restored at the end. Keep the pointer away from the window while it runs.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
web=${NUMR_WEB_DIR:-$root/numr-web}
out=$root/screenshots
work=${TMPDIR:-/tmp}/numr-capture.$$
mkdir -p "$work" "$out"

win_x=100 win_y=100 png_w=960 png_h=600 gif_w=960 gif_h=460
# ydotool absolute coordinates are logical pixels / 2 on this display.
ydo() { sudo ydotool "$@"; }
at() { ydo mousemove -a -x $(( ($1) / 2 )) -y $(( ($2) / 2 )); sleep 0.25; }
click() { ydo click 0xC0; }
numr_addr() { hyprctl clients -j | jq -r '.[] | select(.class=="numr-desktop") | .address'; }
active_is_numr() { [ "$(hyprctl activewindow -j | jq -r .class)" = numr-desktop ]; }
# Focus follows the mouse here, so re-focus explicitly right before any keyboard input.
focus_numr() {
	dsp "hl.dsp.focus({ window = \"address:$addr\" })"; sleep 0.4
	active_is_numr || { echo "numr window is not focused; keep the pointer away from other windows" >&2; exit 1; }
}
dsp() { hyprctl dispatch "$1" > /dev/null; }

cleanup() {
	pkill -INT wf-recorder 2>/dev/null || true
	pkill -x numr-desktop 2>/dev/null || true
	hyprctl switchxkblayout ydotoold-virtual-device 1 > /dev/null 2>&1 || true
	hyprctl switchxkblayout keyd-virtual-keyboard 1 > /dev/null 2>&1 || true
	hyprctl reload > /dev/null 2>&1 || true
	rm -rf "$work"
}
trap cleanup EXIT INT TERM

# 1. Opaque UI copy: Hyprland shows the wallpaper through transparent corners.
ui=$work/ui
mkdir -p "$ui/pkg"
cp "$web/app.js" "$web/style.css" "$web/numr-wordmark.svg" "$ui/"
cp -r "$web/desktop" "$web/pkg/core" "$web/pkg/editor" "$ui/"
mv "$ui/core" "$ui/editor" "$ui/pkg/"
printf '\nhtml, body.desktop { background: var(--numr-bg-primary); }\nbody.desktop .container { border-radius: 0; }\n' >> "$ui/desktop/desktop.css"

# 2. Run the app with that UI and an opaque window.
pkill -x numr-desktop 2>/dev/null || true
( cd "$root" && NUMR_WEB_DIR=$ui cargo tauri dev --config \
	'{"app":{"windows":[{"label":"main","title":"numr","width":960,"height":600,"minWidth":480,"minHeight":320,"center":true,"decorations":false,"transparent":false}]}}' \
	> "$work/tauri.log" 2>&1 ) &
until [ -n "$(numr_addr)" ]; do sleep 1; done
sleep 5
addr=$(numr_addr)

place() { # width height; Hyprland re-centers after a resize, so move and verify afterwards
	for _ in 1 2 3 4 5; do
		floating=$(hyprctl clients -j | jq -r ".[] | select(.address==\"$addr\") | .floating")
		echo "place: floating=$floating" >&2
		[ "$floating" = true ] || dsp "hl.dsp.window.float({ window = \"address:$addr\", action = \"toggle\" })"
		sleep 0.5
		dsp "hl.dsp.window.resize({ window = \"address:$addr\", x = $1, y = $2 })"
		sleep 0.8
		dsp "hl.dsp.window.move({ window = \"address:$addr\", x = $win_x, y = $win_y })"
		sleep 0.6
		pos=$(hyprctl clients -j | jq -r ".[] | select(.address==\"$addr\") | \"\\(.at[0]),\\(.at[1]) \\(.size[0])x\\(.size[1])\"")
		[ "$pos" = "$win_x,$win_y $1x$2" ] && return 0
	done
	echo "window did not settle at $win_x,$win_y $1x$2 (got $pos)" >&2
	exit 1
}
place $png_w $png_h
dsp "hl.dsp.window.set_prop({ window = \"address:$addr\", prop = \"rounding\", value = 0 })"
hyprctl keyword general:border_size 0 > /dev/null
hyprctl keyword input:accel_profile flat > /dev/null
hyprctl keyword cursor:hide_on_key_press true > /dev/null
hyprctl switchxkblayout keyd-virtual-keyboard 0 > /dev/null
hyprctl switchxkblayout ydotoold-virtual-device 0 > /dev/null
sleep 0.5

header_x=$((win_x + 700)) header_y=$((win_y + 18))   # empty drag area: pointer parks here
editor_x=$((win_x + 400)) editor_y=$((win_y + 300))
new_x=$((win_x + 880)) new_y=$((win_y + 18))         # "New document" header button

# 3. Screenshot: scroll the sample document to its interesting part.
"$root/target/debug/numr-desktop" "$root/docs/media/screenshot.numr" || true
sleep 1.5
at $editor_x $editor_y; focus_numr; click; sleep 0.3
ydo mousemove -w -x 0 -y 40; sleep 0.4; ydo mousemove -w -x 0 -y -6; sleep 0.6
at $header_x $header_y; focus_numr; ydo key 42:1 42:0; sleep 1.2        # Shift tap hides the cursor
grim -g "$win_x,$win_y ${png_w}x${png_h}" "$out/numr-desktop-linux.png"

# 4. GIF: empty document, live typing, settings panel.
place $gif_w $gif_h
at $new_x $new_y; focus_numr; click; sleep 0.8
at $editor_x $editor_y; click; sleep 0.3
at $header_x $header_y
gear_x=$((win_x + 19)) gear_y=$((win_y + gif_h - 19))
decimals_x=$((win_x + 188)) decimals_y=$((win_y + gif_h - 67))
wf-recorder -g "$win_x,$win_y ${gif_w}x${gif_h}" -f "$work/rec.mp4" -r 30 -c libx264rgb -p crf=8 > "$work/wf.log" 2>&1 &
sleep 1.2
focus_numr
ydo type -d 28 -H 14 -f "$root/docs/media/demo.numr"; sleep 1.6
at $gear_x $gear_y; click; sleep 1.2
at $decimals_x $decimals_y; click; sleep 0.4; ydo type "0"; sleep 1.5; ydo key 14:1 14:0; sleep 1.0
at $header_x $header_y; click; sleep 1.5
pkill -INT wf-recorder; sleep 2

ffmpeg -v error -y -i "$work/rec.mp4" \
	-vf "fps=15,scale=${gif_w}:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=192:stats_mode=diff[p];[s1][p]paletteuse=dither=none:diff_mode=rectangle" \
	"$out/numr-desktop-demo.gif"
ls -la "$out"
