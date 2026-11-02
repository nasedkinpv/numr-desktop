#!/bin/sh
# Assemble the webview bundle (dist/) from a numr-web checkout.
# Default: the numr-web submodule. Override for side-by-side work:
#   NUMR_WEB_DIR=../numr-web cargo tauri dev
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
web=${NUMR_WEB_DIR:-$root/numr-web}
out=$root/dist

if [ ! -f "$web/app.js" ]; then
	echo "numr-web not found at $web (run: git submodule update --init)" >&2
	exit 1
fi

rm -rf "$out"
mkdir -p "$out/pkg/core" "$out/pkg/editor"
cp "$web/desktop/index.html" "$web/desktop/index.js" "$web/desktop/desktop.css" \
	"$web/app.js" "$web/style.css" "$web/numr-wordmark.svg" "$out/"
cp "$web/pkg/core/numr_core.js" "$web/pkg/core/numr_core_bg.wasm" "$out/pkg/core/"
cp "$web/pkg/editor/numr_editor.js" "$web/pkg/editor/numr_editor_bg.wasm" "$out/pkg/editor/"
echo "ui: $out <- $web"
