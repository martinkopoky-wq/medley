#!/usr/bin/env bash
# Build first with cargo build --release --locked, then run this script.
set -euo pipefail

repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
binary=$(realpath "${1:-$repo/target/release/medley}")
version=${2:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/Cargo.toml")}
version=${version#v}
if [[ $(uname -m) != x86_64 || ! -x "$binary" || ! $version =~ ^[0-9A-Za-z][0-9A-Za-z.+-]*$ ]]; then
    echo 'Usage: appimage.sh /path/to/linux-x86_64/medley [version]' >&2
    exit 1
fi

mkdir -p "$repo/target/appimage/tools" "$repo/dist"
work=$(mktemp -d "$repo/target/appimage/build.XXXXXX")
appdir="$work/Medley.AppDir"
tools="$repo/target/appimage/tools"

# Fixed upstream releases and SHA-256 hashes keep packaging inputs reproducible.
download() {
    local name=$1 url=$2 checksum=$3
    if [[ ! -f "$tools/$name" ]]; then
        curl --fail --location --retry 3 "$url" -o "$work/$name"
        printf '%s  %s\n' "$checksum" "$work/$name" | sha256sum --check
        mv "$work/$name" "$tools/$name"
    fi
    printf '%s  %s\n' "$checksum" "$tools/$name" | sha256sum --check
    chmod +x "$tools/$name"
}

download linuxdeploy-x86_64.AppImage \
    https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage \
    c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d
download appimagetool-x86_64.AppImage \
    https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage \
    ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
download runtime-x86_64 \
    https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64 \
    2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d

# Extract the tools so packaging itself works without a FUSE mount.
mkdir "$work/linuxdeploy" "$work/appimagetool"
(cd "$work/linuxdeploy" && "$tools/linuxdeploy-x86_64.AppImage" --appimage-extract > /dev/null)
(cd "$work/appimagetool" && "$tools/appimagetool-x86_64.AppImage" --appimage-extract > /dev/null)
"$work/linuxdeploy/squashfs-root/AppRun" \
    --appdir "$appdir" \
    --executable "$binary" \
    --desktop-file "$repo/packaging/linux/app.medley.Medley.desktop" \
    --icon-file "$repo/assets/icon-256.png" --icon-filename app.medley.Medley

install -Dm644 "$repo/LICENSE-MIT" "$appdir/usr/share/licenses/medley/LICENSE-MIT"
install -Dm644 "$repo/LICENSE-APACHE" "$appdir/usr/share/licenses/medley/LICENSE-APACHE"
install -Dm644 "$repo/assets/fonts/Inter-LICENSE.txt" "$appdir/usr/share/licenses/medley/Inter-LICENSE.txt"

output="$repo/dist/Medley-$version-x86_64.AppImage"
ARCH=x86_64 VERSION="$version" "$work/appimagetool/squashfs-root/AppRun" \
    --runtime-file "$tools/runtime-x86_64" "$appdir" "$output"
chmod +x "$output"
(cd "$repo/dist" && sha256sum "$(basename "$output")" > "$(basename "$output").sha256")
printf 'AppImage: %s\n' "$output"
