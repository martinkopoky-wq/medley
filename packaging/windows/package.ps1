param([Parameter(Mandatory)][string]$Binary, [Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = 'Stop'
$Version = $Version.TrimStart('v')
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Expected a stable release version' }
$repo = (Resolve-Path "$PSScriptRoot/../..").Path
$payload = "$repo/target/installers/windows-payload"
New-Item -ItemType Directory -Force $payload, "$repo/dist" | Out-Null
Copy-Item $Binary "$payload/medley.exe" -Force
Copy-Item "$repo/LICENSE-MIT", "$repo/LICENSE-APACHE" $payload -Force
# The MSVC binary imports VCRUNTIME140.dll. Ship Microsoft's redistributable
# app-local CRT beside it; do not depend on a developer's system installation.
$vswhere = "${env:ProgramFiles(x86)}/Microsoft Visual Studio/Installer/vswhere.exe"
$vs = & $vswhere -latest -products '*' -property installationPath
$crt = Get-ChildItem "$vs/VC/Redist/MSVC/*/x64/Microsoft.VC*.CRT" -Directory |
  Sort-Object FullName -Descending | Select-Object -First 1
if (!$crt) { throw 'Visual C++ redistributable CRT directory missing' }
Copy-Item "$($crt.FullName)/*.dll" $payload -Force
if (!(Test-Path "$payload/vcruntime140.dll")) { throw 'vcruntime140.dll missing' }
$deletes = "$repo/target/installers/delete-runtime.nsh"
Get-ChildItem "$payload/*.dll" | ForEach-Object { 'Delete "$INSTDIR\' + $_.Name + '"' } | Set-Content $deletes
$nsis = "${env:ProgramFiles(x86)}/NSIS/makensis.exe"
if (!(Test-Path $nsis)) {
  # Runner images vary; fall back to a makensis on PATH (choco install nsis).
  $found = Get-Command makensis.exe -ErrorAction SilentlyContinue
  if ($found) { $nsis = $found.Source } else { throw 'Install NSIS before packaging' }
}
& $nsis "/DVERSION=$Version" "/DPAYLOAD=$payload" "/DDELETE_RUNTIME=$deletes" "/DOUTPUT=$repo/dist/Medley-$Version-x64-Setup.exe" "$PSScriptRoot/installer.nsi"
if ($LASTEXITCODE -ne 0) { throw 'NSIS failed' }
# Standalone binary is useful where the VC++ runtime is already installed.
Copy-Item $Binary "$repo/dist/Medley-$Version-x64.exe" -Force
