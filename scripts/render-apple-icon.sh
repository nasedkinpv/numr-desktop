#!/bin/sh
# Render numr-apple's Icon Composer icon (AppIcon.icon) into icons/ for Tauri.
#
# Runs ON macOS 26+ with Xcode (actool, swiftc). From this repository on the Mac:
#   scripts/render-apple-icon.sh ../numr-apple/Numr/AppIcon.icon
# then `cargo tauri icon /tmp/numr-icon/icon-1024.png -o icons` here or on Linux, and
# delete the generated android/, ios/, Square*.png, StoreLogo.png, icon.ico.
#
# actool compiles the .icon into an asset catalog; IconServices then renders the
# layered icon at any size when asked for a bundle's icon, so a stub .app bundle with
# that catalog is enough to get a 1024px PNG with the same look as Finder shows.
set -eu

icon=${1:?path to AppIcon.icon}
work=/tmp/numr-icon
here=$(cd "$(dirname "$0")" && pwd)
rm -rf "$work" && mkdir -p "$work/out" "$work/numr.app/Contents/MacOS" "$work/numr.app/Contents/Resources"

xcrun actool "$icon" --compile "$work/out" --app-icon AppIcon --platform macosx \
	--minimum-deployment-target 26.0 --output-partial-info-plist "$work/out/partial.plist" > /dev/null
cp "$work/out/Assets.car" "$work/out/AppIcon.icns" "$work/numr.app/Contents/Resources/"
printf '#!/bin/sh\nexit 0\n' > "$work/numr.app/Contents/MacOS/numr"
chmod +x "$work/numr.app/Contents/MacOS/numr"
cat > "$work/numr.app/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>cc.numr.iconrender</string>
<key>CFBundleName</key><string>numr</string>
<key>CFBundleExecutable</key><string>numr</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleIconName</key><string>AppIcon</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>LSMinimumSystemVersion</key><string>26.0</string>
</dict></plist>
EOF

xcrun swiftc -O -o "$work/render-icon" "$here/render-icon.swift"
"$work/render-icon" "$work/numr.app" "$work/icon-1024.png" 1024
echo "rendered $work/icon-1024.png"
