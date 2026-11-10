#!/usr/bin/env bash
# Build a matched modern keyboard library pair without installing into the OS.
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
root="$repo/target/appimage/xkbcommon-1.11.0"
prefix="$root/prefix"
archive="$repo/target/appimage/sources/libxkbcommon-1.11.0.tar.gz"
checksum=8a912a81f27c0c102e2625f60278f36f96afe9fb0a56688a32733103d385f1dc
if [[ -f "$prefix/lib/libxkbcommon.so.0" && -f "$prefix/lib/libxkbcommon-x11.so.0" && -f "$root/complete" ]]; then
    exit 0
fi
for tool in meson ninja bison pkg-config; do
    command -v "$tool" > /dev/null || { echo "Missing $tool: install the AppImage build dependencies." >&2; exit 1; }
done
pkg-config --exists xcb xcb-xkb
mkdir -p "$(dirname "$archive")" "$root/source"
if [[ ! -f "$archive" ]]; then
    curl --fail --location --retry 3 \
        https://codeload.github.com/xkbcommon/libxkbcommon/tar.gz/0f9cefb1fedb23433666fa5d9045e48ff030c006 \
        -o "$archive"
fi
printf '%s  %s\n' "$checksum" "$archive" | sha256sum --check
tar -xzf "$archive" --strip-components=1 -C "$root/source"
if [[ ! -f "$root/build/build.ninja" ]]; then
    meson setup "$root/build" "$root/source" --prefix "$prefix" --libdir lib --buildtype release \
        -Denable-x11=true -Denable-tools=false -Denable-docs=false -Denable-wayland=false \
        -Denable-xkbregistry=false -Denable-bash-completion=false \
        -Dxkb-config-root=/usr/share/X11/xkb -Dx-locale-root=/usr/share/X11/locale
fi
meson compile -C "$root/build" xkbcommon xkbcommon-x11
meson install -C "$root/build" --no-rebuild
touch "$root/complete"
