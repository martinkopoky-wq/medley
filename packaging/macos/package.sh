#!/usr/bin/env bash
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
binary=$(realpath "${1:?Usage: package.sh binary version}")
version=${2#v}
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 1
mkdir -p "$repo/target/installers" "$repo/dist"
work=$(mktemp -d "$repo/target/installers/mac.XXXXXX")
trap 'rm -rf "$work"' EXIT
app="$work/image/Medley.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$work/Medley.iconset"
install -m755 "$binary" "$app/Contents/MacOS/medley"
cp "$repo/LICENSE-MIT" "$repo/LICENSE-APACHE" "$app/Contents/Resources/"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$repo/assets/icon-1024.png" --out "$work/Medley.iconset/icon_${size}x${size}.png" > /dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$repo/assets/icon-1024.png" --out "$work/Medley.iconset/icon_${size}x${size}@2x.png" > /dev/null
done
iconutil -c icns "$work/Medley.iconset" -o "$app/Contents/Resources/Medley.icns"
cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>app.medley.Medley</string>
<key>CFBundleName</key><string>Medley</string>
<key>CFBundleDisplayName</key><string>Medley</string>
<key>CFBundleExecutable</key><string>medley</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>CFBundleIconFile</key><string>Medley</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSApplicationCategoryType</key><string>public.app-category.music</string>
</dict></plist>
EOF
plutil -lint "$app/Contents/Info.plist"
codesign --force --sign - "$app"
codesign --verify --strict "$app"
ln -s /Applications "$work/image/Applications"
hdiutil create -volname Medley -srcfolder "$work/image" -ov -format UDZO "$repo/dist/Medley-$version-arm64.dmg"
ditto -c -k --sequesterRsrc --keepParent "$app" "$repo/dist/Medley-$version-arm64.app.zip"
