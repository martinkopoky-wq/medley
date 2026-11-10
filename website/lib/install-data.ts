/** Release names follow .github/workflows/release.yml and packaging/linux/appimage.sh. */
export const RELEASE_VERSION = '0.1.4';
export const REPOSITORY = 'https://github.com/martinkopoky-wq/medley';
export const RELEASE_URL = `${REPOSITORY}/releases/tag/v${RELEASE_VERSION}`;
const downloadBase = `${REPOSITORY}/releases/download/v${RELEASE_VERSION}`;

export type InstallPlatform = 'linux' | 'macos' | 'windows' | 'source';
export interface InstallOption {
  id: InstallPlatform;
  label: string;
  architecture: string;
  requirements: string;
  downloadUrl: string;
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
    requirements: 'Mac mit Apple Silicon, curl und tar. Für Intel-Macs bitte aus dem Quellcode bauen.',
    downloadUrl: `${downloadBase}/medley-v${RELEASE_VERSION}-aarch64-apple-darwin.tar.gz`,
    shell: 'Bash / Zsh',
    installPath: '~/Applications/Medley/medley',
    command: `(
  set -eu
  version='${RELEASE_VERSION}'
  base='${REPOSITORY}/releases/download'
  file="medley-v$version-aarch64-apple-darwin.tar.gz"
  workdir="$(mktemp -d)"
  trap 'rm -rf "$workdir"' EXIT
  curl -fL --retry 3 "$base/v$version/$file" -o "$workdir/$file"
  mkdir -p "$HOME/Applications/Medley"
  tar -xzf "$workdir/$file" --strip-components=1 -C "$HOME/Applications/Medley"
  chmod +x "$HOME/Applications/Medley/medley"
  printf '\\nInstalliert: %s\\nStarten: "%s"\\n' "$HOME/Applications/Medley/medley" "$HOME/Applications/Medley/medley"
)`,
    notes: [
      'Starten im Terminal mit "$HOME/Applications/Medley/medley".',
      'Das Release ist eine native Programmdatei im tar.gz-Archiv; es ist kein signiertes oder notarisiertes .app-Paket.',
      'macOS kann beim ersten Öffnen eine Freigabe in Systemeinstellungen → Datenschutz & Sicherheit verlangen. Die Systemsicherheit muss dafür nicht deaktiviert werden.',
      'Für dieses Archiv veröffentlicht der Release-Workflow derzeit keine separate Prüfsumme.',
    ],
  },
  {
    id: 'windows',
    label: 'Windows',
    architecture: 'Intel / AMD · x64',
    requirements: 'Windows x64 mit PowerShell 5.1 oder neuer. Installation im eigenen Benutzerkonto, ohne Administratorrechte.',
    downloadUrl: `${downloadBase}/medley-v${RELEASE_VERSION}-x86_64-pc-windows-msvc.zip`,
    shell: 'PowerShell',
    installPath: '%LOCALAPPDATA%\\Programs\\Medley\\medley.exe',
    command: `$ErrorActionPreference = 'Stop'
$version = '${RELEASE_VERSION}'
$base = '${REPOSITORY}/releases/download'
$name = "medley-v$version-x86_64-pc-windows-msvc"
$workdir = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
$destination = Join-Path $env:LOCALAPPDATA 'Programs\\Medley'
try {
  New-Item -ItemType Directory -Force -Path $workdir, $destination | Out-Null
  $archive = Join-Path $workdir "$name.zip"
  Invoke-WebRequest -UseBasicParsing -Uri "$base/v$version/$name.zip" -OutFile $archive
  Expand-Archive -LiteralPath $archive -DestinationPath $workdir -Force
  Copy-Item -Path (Join-Path $workdir "$name\\*") -Destination $destination -Recurse -Force
  Write-Host "Installiert: $(Join-Path $destination 'medley.exe')"
} finally {
  if (Test-Path -LiteralPath $workdir) { Remove-Item -LiteralPath $workdir -Recurse -Force }
}
# Medley starten:
& (Join-Path $destination 'medley.exe')`,
    notes: [
      'Später per Doppelklick auf medley.exe im Installationsordner starten.',
      'Der Befehl entpackt das ZIP und kopiert den Inhalt des Versionsordners an den endgültigen Ort.',
      'Das Release ist ein portables Programm, kein MSI-Installer. Für das ZIP gibt es derzeit keine separate Release-Prüfsumme.',
    ],
  },
  {
    id: 'source',
    label: 'Quellcode',
    architecture: 'Linux · macOS · Windows',
    requirements: 'Git, Rust ≥ 1.88 und Cargo. Linux: C-Toolchain, pkg-config, ALSA- und Grafikbibliotheken. macOS: Xcode Command Line Tools. Windows: MSVC C++ Build Tools mit Windows SDK.',
    downloadUrl: `${REPOSITORY}/archive/refs/tags/v${RELEASE_VERSION}.tar.gz`,
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
