/** Release names follow .github/workflows/release.yml, package-installers.yml and packaging/. */
export const RELEASE_VERSION = '0.1.5';
export const REPOSITORY = 'https://github.com/martinkopoky-wq/medley';
export const RELEASE_URL = `${REPOSITORY}/releases/tag/v${RELEASE_VERSION}`;
const downloadBase = `${REPOSITORY}/releases/download/v${RELEASE_VERSION}`;

export type InstallPlatform = 'linux' | 'macos' | 'windows' | 'source';
export interface InstallFormat {
  label: string;
  note: string;
  url: string;
}
export interface InstallOption {
  id: InstallPlatform;
  label: string;
  architecture: string;
  requirements: string;
  downloadUrl: string;
  formats: readonly InstallFormat[];
  command: string;
  installPath: string;
  shell: 'Bash / Zsh' | 'PowerShell';
  notes: readonly string[];
  fallbackCommand?: string;
  fallbackLabel?: string;
  prerequisites?: readonly { label: string; command: string }[];
}

export const INSTALL_OPTIONS: readonly InstallOption[] = [
  {
    id: 'linux',
    label: 'Linux',
    architecture: 'Intel / AMD · x86_64',
    requirements: 'Linux-Desktop mit glibc ≥ 2.35 (z. B. Ubuntu 22.04+), curl und sha256sum. FUSE oder der alternative Startbefehl unten.',
    downloadUrl: `${downloadBase}/Medley-${RELEASE_VERSION}-x86_64.AppImage`,
    formats: [
      { label: 'AppImage', note: 'Einzelne Datei, startet ohne Installation', url: `${downloadBase}/Medley-${RELEASE_VERSION}-x86_64.AppImage` },
      { label: '.deb · Debian / Ubuntu', note: 'Mit Menüeintrag und Entfernungsanleitung', url: `${downloadBase}/medley_${RELEASE_VERSION}_amd64.deb` },
      { label: 'tar.gz', note: 'Programmdatei und Daten zum Selbsteinrichten', url: `${downloadBase}/medley-v${RELEASE_VERSION}-x86_64-unknown-linux-gnu.tar.gz` },
    ],
    shell: 'Bash / Zsh',
    installPath: '~/.local/bin/medley',
    command: `(
  set -eu
  version='${RELEASE_VERSION}'
  base='${REPOSITORY}/releases/download'
  file="Medley-$version-x86_64.AppImage"
  workdir="$(mktemp -d)"
  trap 'rm -rf "$workdir"' EXIT
  cd "$workdir"
  curl -fL --retry 3 "$base/v$version/$file" -o "$file"
  curl -fL --retry 3 "$base/v$version/$file.sha256" -o "$file.sha256"
  sha256sum --check "$file.sha256"
  install -Dm755 "$file" "$HOME/.local/bin/medley"
  printf '\\nInstalliert: %s\\nStarten: "%s"\\n' "$HOME/.local/bin/medley" "$HOME/.local/bin/medley"
)`,
    fallbackLabel: 'Start ohne FUSE',
    fallbackCommand: '"$HOME/.local/bin/medley" --appimage-extract-and-run',
    notes: [
      'Starten mit ~/.local/bin/medley. Liegt ~/.local/bin in deinem PATH, reicht medley.',
      'Die Prüfsumme wird vor der Installation kontrolliert. Bei einem Download- oder Prüffehler bricht der Befehl ab.',
      'Das AppImage enthält Spotify-Wiedergabe. Eine Linux-ARM-Version wird aktuell nicht als fertiger Download gebaut.',
    ],
  },
  {
    id: 'macos',
    label: 'macOS',
    architecture: 'Apple Silicon · arm64',
    requirements: 'Mac mit Apple Silicon, macOS 11 oder neuer, curl und shasum. Für Intel-Macs bitte aus dem Quellcode bauen.',
    downloadUrl: `${downloadBase}/Medley-${RELEASE_VERSION}-arm64.dmg`,
    formats: [
      { label: '.dmg', note: 'Festplatte einlegen, Medley.app in Programme ziehen', url: `${downloadBase}/Medley-${RELEASE_VERSION}-arm64.dmg` },
      { label: 'Medley.app als ZIP', note: 'Dieselbe App ohne DMG-Hülle', url: `${downloadBase}/Medley-${RELEASE_VERSION}-arm64.app.zip` },
      { label: 'tar.gz', note: 'Nackte Programmdatei für Skripte', url: `${downloadBase}/medley-v${RELEASE_VERSION}-aarch64-apple-darwin.tar.gz` },
    ],
    shell: 'Bash / Zsh',
    installPath: '~/Applications/Medley.app',
    command: `(
  set -eu
  version='${RELEASE_VERSION}'
  base='${REPOSITORY}/releases/download'
  file="Medley-$version-arm64.dmg"
  workdir="$(mktemp -d)"
  trap 'rm -rf "$workdir"' EXIT
  curl -fL --retry 3 "$base/v$version/$file" -o "$workdir/$file"
  curl -fL --retry 3 "$base/v$version/$file.sha256" -o "$workdir/$file.sha256"
  ( cd "$workdir" && shasum -a 256 --check "$file.sha256" )
  hdiutil attach -nobrowse -quiet "$workdir/$file"
  ditto "/Volumes/Medley/Medley.app" "$HOME/Applications/Medley.app"
  hdiutil detach -quiet "/Volumes/Medley"
  printf '\\nInstalliert: %s\\n' "$HOME/Applications/Medley.app"
)`,
    notes: [
      'Die App liegt danach in ~/Applications und startet über Launchpad oder Spotlight.',
      'Das DMG ist ad-hoc signiert, aber nicht von Apple notariell geprüft. macOS fragt deshalb beim ersten Start; bestätige mit „Öffnen“ (Rechtsklick → Öffnen geht auch).',
      'Der Befehl prüft die SHA-256-Prüfsumme des DMG, bevor er es einhängt.',
      'Für Intel-Macs gibt es noch keinen fertigen Download; dort aktuell aus dem Quellcode bauen.',
    ],
  },
  {
    id: 'windows',
    label: 'Windows',
    architecture: 'Intel / AMD · x64',
    requirements: 'Windows 10 oder 11, x64, PowerShell 5.1 oder neuer. Installation im eigenen Benutzerkonto, ohne Administratorrechte.',
    downloadUrl: `${downloadBase}/Medley-${RELEASE_VERSION}-x64-Setup.exe`,
    formats: [
      { label: 'Setup.exe', note: 'Installer mit Startmenü-Eintrag und Deinstallation', url: `${downloadBase}/Medley-${RELEASE_VERSION}-x64-Setup.exe` },
      { label: 'Medley.exe · portabel', note: 'Einzelne Datei, braucht die Visual-C++-Laufzeit im System', url: `${downloadBase}/Medley-${RELEASE_VERSION}-x64.exe` },
      { label: 'ZIP', note: 'Alle Dateien entpackt zum Selbsteinrichten', url: `${downloadBase}/medley-v${RELEASE_VERSION}-x86_64-pc-windows-msvc.zip` },
    ],
    shell: 'PowerShell',
    installPath: '%LOCALAPPDATA%\\Programs\\Medley\\medley.exe',
    command: `$ErrorActionPreference = 'Stop'
$version = '${RELEASE_VERSION}'
$base = '${REPOSITORY}/releases/download'
$name = "Medley-$version-x64-Setup.exe"
$workdir = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
try {
  New-Item -ItemType Directory -Force -Path $workdir | Out-Null
  $installer = Join-Path $workdir $name
  Invoke-WebRequest -UseBasicParsing -Uri "$base/v$version/$name" -OutFile $installer
  $hash = (Get-FileHash -Algorithm SHA256 $installer).Hash.ToLower()
  $expected = (Invoke-WebRequest -UseBasicParsing "$base/v$version/$name.sha256").Content.Split(' ')[0]
  if ($hash -ne $expected) { throw 'Prüfsumme stimmt nicht.' }
  Start-Process -FilePath $installer -ArgumentList '/S' -Wait
  Write-Host "Installiert: $(Join-Path $env:LOCALAPPDATA 'Programs\\Medley\\medley.exe')"
} finally {
  if (Test-Path -LiteralPath $workdir) { Remove-Item -LiteralPath $workdir -Recurse -Force }
}
# Medley starten:
& (Join-Path $env:LOCALAPPDATA 'Programs\\Medley\\medley.exe')`,
    notes: [
      'Der Installer trägt Medley ins Startmenü ein und lässt sich über „Apps & Features“ wieder entfernen. Eigene Einstellungen bleiben dabei erhalten.',
      'Der Befehl vergleicht die SHA-256-Prüfsumme der Setup.exe, bevor er sie startet.',
      'Der Installer legt die nötige Microsoft-Visual-C++-Laufzeit direkt neben dem Programm ab.',
      'Ohne Installer: die portable Medley.exe oder das ZIP verwenden; dort muss die Visual-C++-Laufzeit im System vorhanden sein.',
    ],
  },
  {
    id: 'source',
    label: 'Quellcode',
    architecture: 'Linux · macOS · Windows',
    requirements: 'Git, Rust ≥ 1.88 und Cargo. Linux: C-Toolchain, pkg-config, ALSA- und Grafikbibliotheken. macOS: Xcode Command Line Tools. Windows: MSVC C++ Build Tools mit Windows SDK.',
    downloadUrl: `${REPOSITORY}/archive/refs/tags/v${RELEASE_VERSION}.tar.gz`,
    formats: [
      { label: 'Quellcode · tar.gz', note: `Der Stand von v${RELEASE_VERSION}`, url: `${REPOSITORY}/archive/refs/tags/v${RELEASE_VERSION}.tar.gz` },
    ],
    shell: 'Bash / Zsh',
    installPath: '~/.local/bin/medley (Linux/macOS); %LOCALAPPDATA%\\Programs\\Medley\\medley.exe (Windows)',
    command: `(
  set -eu
  version='${RELEASE_VERSION}'
  mkdir -p "$HOME/src"
  git clone --branch "v$version" --depth 1 '${REPOSITORY}.git' "$HOME/src/medley-v$version"
  cd "$HOME/src/medley-v$version"
  cargo build --release --locked -p medley
  mkdir -p "$HOME/.local/bin"
  cp target/release/medley "$HOME/.local/bin/medley"
  chmod +x "$HOME/.local/bin/medley"
  printf '\\nInstalliert: %s\\nStarten: "%s"\\n' "$HOME/.local/bin/medley" "$HOME/.local/bin/medley"
)`,
    fallbackLabel: 'Quellcode unter Windows (PowerShell)',
    fallbackCommand: `$ErrorActionPreference = 'Stop'
$version = '${RELEASE_VERSION}'
$source = Join-Path $HOME "src\\medley-v$version"
New-Item -ItemType Directory -Force -Path (Split-Path $source -Parent) | Out-Null
git clone --branch "v$version" --depth 1 '${REPOSITORY}.git' $source
if ($LASTEXITCODE -ne 0) { throw 'Git-Download fehlgeschlagen.' }
Push-Location $source
try {
  cargo build --release --locked -p medley
  if ($LASTEXITCODE -ne 0) { throw 'Cargo-Build fehlgeschlagen.' }
  $destination = Join-Path $env:LOCALAPPDATA 'Programs\\Medley'
  New-Item -ItemType Directory -Force -Path $destination | Out-Null
  Copy-Item 'target\\release\\medley.exe' (Join-Path $destination 'medley.exe') -Force
  Write-Host "Installiert: $(Join-Path $destination 'medley.exe')"
} finally {
  Pop-Location
}`,
    prerequisites: [
      { label: 'Debian / Ubuntu', command: 'sudo apt update\nsudo apt install -y git build-essential pkg-config libasound2-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-0 libgl1-mesa-dev' },
      { label: 'Fedora', command: 'sudo dnf install -y git gcc pkgconf-pkg-config alsa-lib-devel wayland-devel libxkbcommon-devel libxkbcommon-x11 mesa-libGL-devel' },
      { label: 'Arch Linux', command: 'sudo pacman -S --needed git base-devel pkgconf alsa-lib wayland libxkbcommon libxkbcommon-x11 mesa' },
      { label: 'macOS', command: 'xcode-select --install' },
    ],
    notes: [
      'Zuerst die Voraussetzungen für dein System installieren. Rust muss bereits installiert sein; rustup update stable aktualisiert eine vorhandene rustup-Installation.',
      `Der Quellcode liegt anschließend in ~/src/medley-v${RELEASE_VERSION}, das gebaute Programm zusätzlich in target/release/medley (Windows: medley.exe).`,
      'Die Befehle bauen das konkrete Release mit Cargo.lock und eingeschalteter Spotify-Wiedergabe. Ein bereits vorhandener Quellcode-Zielordner wird nicht überschrieben.',
    ],
  },
];

export const INSTALL_DATA = Object.fromEntries(
  INSTALL_OPTIONS.map((option) => [option.id, option]),
) as Record<InstallPlatform, InstallOption>;
