# Medley installieren und die Website lokal starten

Medley ist ein natives Desktop-Programm. Die Produktwebsite zeigt Funktionen und Downloads; sie spielt selbst keine Musik ab. Die Desktop-Version benötigt weder Node.js noch einen Webserver.

Die folgenden Download-Befehle sind auf **v0.1.4** festgelegt. Sie funktionieren, sobald dieses Release inklusive aller Dateien auf GitHub veröffentlicht ist. Eine Änderung von RELEASE_VERSION in website/lib/install-data.ts aktualisiert die Download-Links und Befehle der Website zentral; dieses Dokument muss bei Versionswechseln ebenfalls aktualisiert werden.

Repository: [Medley](https://github.com/martinkopoky-wq/medley). Releases: [v0.1.4](https://github.com/martinkopoky-wq/medley/releases/tag/v0.1.4).

## Desktop installieren

Wähle nur den Abschnitt für dein Betriebssystem. Die Befehle installieren im eigenen Benutzerkonto. Schließe Medley, bevor du eine vorhandene Installation aktualisierst.

### Linux · Intel / AMD · x86_64

Linux-Desktop mit glibc ≥ 2.35 (z. B. Ubuntu 22.04+), curl und sha256sum. FUSE oder der alternative Startbefehl unten.

Installationspfad: `~/.local/bin/medley`.

[Direkter Download](https://github.com/martinkopoky-wq/medley/releases/download/v0.1.4/Medley-0.1.4-x86_64.AppImage)

**Bash / Zsh**

```sh
(
  set -eu
  version='0.1.4'
  base='https://github.com/martinkopoky-wq/medley/releases/download'
  file="Medley-$version-x86_64.AppImage"
  workdir="$(mktemp -d)"
  trap 'rm -rf "$workdir"' EXIT
  cd "$workdir"
  curl -fL --retry 3 "$base/v$version/$file" -o "$file"
  curl -fL --retry 3 "$base/v$version/$file.sha256" -o "$file.sha256"
  sha256sum --check "$file.sha256"
  install -Dm755 "$file" "$HOME/.local/bin/medley"
  printf '\nInstalliert: %s\nStarten: "%s"\n' "$HOME/.local/bin/medley" "$HOME/.local/bin/medley"
)
```

**Start ohne FUSE**

```sh
"$HOME/.local/bin/medley" --appimage-extract-and-run
```

- Starten mit ~/.local/bin/medley. Liegt ~/.local/bin in deinem PATH, reicht medley.
- Die Prüfsumme wird vor der Installation kontrolliert. Bei einem Download- oder Prüffehler bricht der Befehl ab.
- Das AppImage enthält Spotify-Wiedergabe. Eine Linux-ARM-Version wird aktuell nicht als fertiger Download gebaut.

### macOS · Apple Silicon · arm64

Mac mit Apple Silicon, curl und tar. Für Intel-Macs bitte aus dem Quellcode bauen.

Installationspfad: `~/Applications/Medley/medley`.

[Direkter Download](https://github.com/martinkopoky-wq/medley/releases/download/v0.1.4/medley-v0.1.4-aarch64-apple-darwin.tar.gz)

**Bash / Zsh**

```sh
(
  set -eu
  version='0.1.4'
  base='https://github.com/martinkopoky-wq/medley/releases/download'
  file="medley-v$version-aarch64-apple-darwin.tar.gz"
  workdir="$(mktemp -d)"
  trap 'rm -rf "$workdir"' EXIT
  curl -fL --retry 3 "$base/v$version/$file" -o "$workdir/$file"
  mkdir -p "$HOME/Applications/Medley"
  tar -xzf "$workdir/$file" --strip-components=1 -C "$HOME/Applications/Medley"
  chmod +x "$HOME/Applications/Medley/medley"
  printf '\nInstalliert: %s\nStarten: "%s"\n' "$HOME/Applications/Medley/medley" "$HOME/Applications/Medley/medley"
)
```

- Starten im Terminal mit "$HOME/Applications/Medley/medley".
- Das Release ist eine native Programmdatei im tar.gz-Archiv; es ist kein signiertes oder notarisiertes .app-Paket.
- macOS kann beim ersten Öffnen eine Freigabe in Systemeinstellungen → Datenschutz & Sicherheit verlangen. Die Systemsicherheit muss dafür nicht deaktiviert werden.
- Für dieses Archiv veröffentlicht der Release-Workflow derzeit keine separate Prüfsumme.

### Windows · Intel / AMD · x64

Windows x64 mit PowerShell 5.1 oder neuer. Installation im eigenen Benutzerkonto, ohne Administratorrechte.

Installationspfad: `%LOCALAPPDATA%\Programs\Medley\medley.exe`.

[Direkter Download](https://github.com/martinkopoky-wq/medley/releases/download/v0.1.4/medley-v0.1.4-x86_64-pc-windows-msvc.zip)

**PowerShell**

```powershell
$ErrorActionPreference = 'Stop'
$version = '0.1.4'
$base = 'https://github.com/martinkopoky-wq/medley/releases/download'
$name = "medley-v$version-x86_64-pc-windows-msvc"
$workdir = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
$destination = Join-Path $env:LOCALAPPDATA 'Programs\Medley'
try {
  New-Item -ItemType Directory -Force -Path $workdir, $destination | Out-Null
  $archive = Join-Path $workdir "$name.zip"
  Invoke-WebRequest -UseBasicParsing -Uri "$base/v$version/$name.zip" -OutFile $archive
  Expand-Archive -LiteralPath $archive -DestinationPath $workdir -Force
  Copy-Item -Path (Join-Path $workdir "$name\*") -Destination $destination -Recurse -Force
  Write-Host "Installiert: $(Join-Path $destination 'medley.exe')"
} finally {
  if (Test-Path -LiteralPath $workdir) { Remove-Item -LiteralPath $workdir -Recurse -Force }
}
# Medley starten:
& (Join-Path $destination 'medley.exe')
```

- Später per Doppelklick auf medley.exe im Installationsordner starten.
- Der Befehl entpackt das ZIP und kopiert den Inhalt des Versionsordners an den endgültigen Ort.
- Das Release ist ein portables Programm, kein MSI-Installer. Für das ZIP gibt es derzeit keine separate Release-Prüfsumme.

### Quellcode · Linux · macOS · Windows

Git, Rust ≥ 1.88 und Cargo. Linux: C-Toolchain, pkg-config, ALSA- und Grafikbibliotheken. macOS: Xcode Command Line Tools. Windows: MSVC C++ Build Tools mit Windows SDK.

Installationspfad: `~/.local/bin/medley (Linux/macOS); %LOCALAPPDATA%\Programs\Medley\medley.exe (Windows)`.

[Direkter Download](https://github.com/martinkopoky-wq/medley/archive/refs/tags/v0.1.4.tar.gz)

Für einen Quellcode-Build zuerst genau die Voraussetzungen deines Systems installieren:

**Debian / Ubuntu**

```sh
sudo apt update
sudo apt install -y git build-essential pkg-config libasound2-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-0 libgl1-mesa-dev
```

**Fedora**

```sh
sudo dnf install -y git gcc pkgconf-pkg-config alsa-lib-devel wayland-devel libxkbcommon-devel libxkbcommon-x11 mesa-libGL-devel
```

**Arch Linux**

```sh
sudo pacman -S --needed git base-devel pkgconf alsa-lib wayland libxkbcommon libxkbcommon-x11 mesa
```

**macOS**

```sh
xcode-select --install
```

Windows benötigt Git, Rust ≥ 1.88 mit MSVC-Toolchain und Visual Studio C++ Build Tools einschließlich Windows SDK. Die folgenden Unix-Befehle setzen bereits installiertes Rust und Cargo voraus; eine vorhandene rustup-Installation lässt sich mit `rustup update stable` aktualisieren.

**Bash / Zsh**

```sh
(
  set -eu
  version='0.1.4'
  mkdir -p "$HOME/src"
  git clone --branch "v$version" --depth 1 'https://github.com/martinkopoky-wq/medley.git' "$HOME/src/medley-v$version"
  cd "$HOME/src/medley-v$version"
  cargo build --release --locked -p medley
  mkdir -p "$HOME/.local/bin"
  cp target/release/medley "$HOME/.local/bin/medley"
  chmod +x "$HOME/.local/bin/medley"
  printf '\nInstalliert: %s\nStarten: "%s"\n' "$HOME/.local/bin/medley" "$HOME/.local/bin/medley"
)
```

**Quellcode unter Windows (PowerShell)**

```powershell
$ErrorActionPreference = 'Stop'
$version = '0.1.4'
$source = Join-Path $HOME "src\medley-v$version"
New-Item -ItemType Directory -Force -Path (Split-Path $source -Parent) | Out-Null
git clone --branch "v$version" --depth 1 'https://github.com/martinkopoky-wq/medley.git' $source
if ($LASTEXITCODE -ne 0) { throw 'Git-Download fehlgeschlagen.' }
Push-Location $source
try {
  cargo build --release --locked -p medley
  if ($LASTEXITCODE -ne 0) { throw 'Cargo-Build fehlgeschlagen.' }
  $destination = Join-Path $env:LOCALAPPDATA 'Programs\Medley'
  New-Item -ItemType Directory -Force -Path $destination | Out-Null
  Copy-Item 'target\release\medley.exe' (Join-Path $destination 'medley.exe') -Force
  Write-Host "Installiert: $(Join-Path $destination 'medley.exe')"
} finally {
  Pop-Location
}
```

- Zuerst die Voraussetzungen für dein System installieren. Rust muss bereits installiert sein; rustup update stable aktualisiert eine vorhandene rustup-Installation.
- Der Quellcode liegt anschließend in ~/src/medley-v0.1.4, das gebaute Programm zusätzlich in target/release/medley (Windows: medley.exe).
- Die Befehle bauen das konkrete Release mit Cargo.lock und eingeschalteter Spotify-Wiedergabe. Ein bereits vorhandener Quellcode-Zielordner wird nicht überschrieben.

## Wo speichert Medley Einstellungen und Musikdaten?

Die Installationspfade oben enthalten das Programm. Einstellungen, Zugangsdaten und lokale Mixes liegen separat in den üblichen Benutzerordnern:

| System | Einstellungen | Mixes und Layouts | Cache |
| --- | --- | --- | --- |
| Linux | `~/.config/medley/` | `~/.local/share/medley/` | `~/.cache/medley/` |
| macOS | `~/Library/Application Support/app.Medley.medley/` | gleicher Ordner | `~/Library/Caches/app.Medley.medley/` |
| Windows | `%APPDATA%\Medley\medley\config\` | `%APPDATA%\Medley\medley\data\` | `%LOCALAPPDATA%\Medley\medley\cache\` |

## Nur für die Entwicklung der Produktwebsite

Node.js **≥ 22.13.0**, npm und Git müssen installiert sein. Diese Befehle erstellen eine separate Arbeitskopie unter `~/src/medley-website`; die Website liegt darin im Unterordner `website`. Sie installieren das Desktop-Programm nicht.

### Website herunterladen und starten (Bash / Zsh)

```sh
mkdir -p "$HOME/src"
git clone 'https://github.com/martinkopoky-wq/medley.git' "$HOME/src/medley-website"
cd "$HOME/src/medley-website/website"
node --version
npm ci
npm run dev
```

Öffne die lokale Adresse, die der Entwicklungsserver im Terminal ausgibt. Beenden mit `Ctrl+C`. Falls das Repository schon vorhanden ist, öffne darin den Unterordner `website` und beginne bei `npm ci`.

### Produktions-Build

```sh
cd "$HOME/src/medley-website/website"
npm run build
```

Der Build schreibt die Website nach `website/dist/` (Worker-Einstieg: `dist/server/index.js`). Der folgende lokale Start verwendet den gebauten Worker:

```sh
cd "$HOME/src/medley-website/website"
npm run start
```

Die Website benötigt keine Spotify-, SoundCloud- oder Apple-Music-Zugangsdaten. Für ihre Installationstexte ist `website/lib/install-data.ts` die zentrale Quelle. Die Archive und ihre internen Versionsordner folgen `.github/workflows/release.yml`; der AppImage-Dateiname und die Prüfsumme folgen `packaging/linux/appimage.sh`. Es gibt derzeit keine offiziellen Homebrew-, Winget- oder APT-Installationspakete.
