#!/usr/bin/env bash
# Reuse the verified AppImage payload, including its matched XKB libraries.
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
image=$(realpath "${1:?Usage: deb.sh AppImage version}")
version=${2#v}
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 1
mkdir -p "$repo/target/installers" "$repo/dist"
work=$(mktemp -d "$repo/target/installers/deb.XXXXXX")
trap 'rm -rf "$work"' EXIT
(cd "$work" && "$image" --appimage-extract > /dev/null)
payload="$work/squashfs-root/usr"
root="$work/package"
install -d "$root/opt/medley" "$root/usr/bin" "$root/DEBIAN"
cp -a "$payload/bin" "$payload/lib" "$root/opt/medley/"
ln -s /opt/medley/bin/medley "$root/usr/bin/medley"
install -Dm644 "$repo/packaging/linux/app.medley.Medley.desktop" "$root/usr/share/applications/app.medley.Medley.desktop"
install -Dm644 "$repo/assets/icon-256.png" "$root/usr/share/icons/hicolor/256x256/apps/app.medley.Medley.png"
install -d "$root/usr/share/doc/medley"
cp -a "$payload/share/licenses/medley/." "$root/usr/share/doc/medley/"
cat > "$root/DEBIAN/control" <<EOF
Package: medley
Version: $version
Architecture: amd64
Maintainer: Medley contributors <martinkopoky@gmail.com>
Section: sound
Priority: optional
Depends: libc6 (>= 2.35), libgcc-s1, libasound2, libgl1, libegl1, libxcb1, libx11-6, libx11-xcb1, libxi6, libxrandr2, libxcursor1, libxinerama1, libxrender1, libwayland-client0, libwayland-cursor0, libwayland-egl1, xkb-data
Installed-Size: $(du -sk "$root/opt" "$root/usr" | awk '{total += $1} END {print total}')
Homepage: https://github.com/martinkopoky-wq/medley
Description: Native music library for Spotify, SoundCloud and Apple Music
 Cross-service mixes, dockable panels and optional AI features.
 Spotify playback requires Premium and a compatible account.
EOF
chmod -R go-w "$root"
dpkg-deb --root-owner-group --build "$root" "$repo/dist/medley_${version}_amd64.deb"
