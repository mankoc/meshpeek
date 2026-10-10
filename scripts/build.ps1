<#
.SYNOPSIS
    Build, test and package MeshPeek on Windows.

.DESCRIPTION
    Runs the checks (format, lint, tests), builds the release executable and writes to dist\:
      - MeshPeek-<version>-windows-x64.zip   portable: unzip and run meshpeek.exe
      - MeshPeek-<version>-x86_64.msi        installer (only if WiX Toolset 3 and cargo-wix are installed)

.PARAMETER Check
    Only run the checks (format, lint, tests); build nothing. Used by CI.
.PARAMETER SkipTests
    Skip the checks and go straight to the release build.
.PARAMETER NoMsi
    Do not build the MSI even if WiX is available.
.PARAMETER InstallTools
    Install the packaging tools first: WiX Toolset 3 (portable binaries, unpacked under %LOCALAPPDATA%, no admin
    rights needed), cargo-wix and cargo-about.

.EXAMPLE
    .\scripts\build.ps1                  # checks + exe + zip + MSI
    .\scripts\build.ps1 -SkipTests       # quick package build
    .\scripts\build.ps1 -InstallTools    # first time on a new machine
#>
param([switch]$Check, [switch]$SkipTests, [switch]$NoMsi, [switch]$InstallTools)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

function Step($text) { Write-Host "`n==> $text" -ForegroundColor Cyan }
# Run a native command given as a script block (arguments like `--` and `-D` reach the program untouched)
# and stop if it fails.
function Run([scriptblock]$Command) {
    & $Command
    if ($LASTEXITCODE -ne 0) { throw "failed (exit $LASTEXITCODE): $Command" }
}

# WiX Toolset 3: an installed copy, or the portable binaries that -InstallTools unpacks per user (no admin needed).
$PortableWix = Join-Path $env:LOCALAPPDATA "MeshPeek-build\wix314"
foreach ($dir in @("${env:ProgramFiles(x86)}\WiX Toolset v3.14\bin", $PortableWix)) {
    if (Test-Path (Join-Path $dir "candle.exe")) { $env:Path = "$dir;$env:Path"; break }
}

if ($InstallTools) {
    Step "Installing packaging tools"
    if (-not (Get-Command candle.exe -ErrorAction SilentlyContinue)) {
        $zip = Join-Path $env:TEMP "wix314-binaries.zip"
        Write-Host "    downloading WiX Toolset 3.14 binaries"
        Invoke-WebRequest "https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip" -OutFile $zip
        Expand-Archive $zip -DestinationPath $PortableWix -Force
        Remove-Item $zip
        $env:Path = "$PortableWix;$env:Path"
    }
    if (-not (Get-Command cargo-wix -ErrorAction SilentlyContinue)) { Run { cargo install cargo-wix --locked } }
    if (-not (Get-Command cargo-about -ErrorAction SilentlyContinue)) { Run { cargo install cargo-about --locked --features cli } }
}

if (-not $SkipTests) {
    Step "Checking formatting"
    Run { cargo fmt --all --check }
    Step "Linting (clippy)"
    Run { cargo clippy --all-targets -- -D warnings }
    Step "Running tests"
    Run { cargo test --all }
}
if ($Check) { Write-Host "`nChecks passed." -ForegroundColor Green; exit 0 }

Step "Building release executable"
Run { cargo build --release -p meshpeek-app }

Step "Collecting third-party licenses"
if (-not (Get-Command cargo-about -ErrorAction SilentlyContinue)) {
    throw "cargo-about not found: run with -InstallTools once"
}
Run { cargo about generate about.hbs -o target\release\THIRD-PARTY-LICENSES.html }
$Version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$Dist = Join-Path $Root "dist"
New-Item -ItemType Directory -Force $Dist | Out-Null

Step "Packaging portable zip"
$Name = "MeshPeek-$Version-windows-x64"
$Stage = Join-Path $Dist $Name
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory $Stage | Out-Null
Copy-Item target\release\meshpeek.exe, target\release\THIRD-PARTY-LICENSES.html, LICENSE, README.md $Stage
Compress-Archive -Path "$Stage\*" -DestinationPath "$Dist\$Name.zip" -Force
Remove-Item -Recurse -Force $Stage
Write-Host "    $Dist\$Name.zip"

if (-not $NoMsi) {
    Step "Building MSI installer"
    if ((Get-Command candle.exe -ErrorAction SilentlyContinue) -and (Get-Command cargo-wix -ErrorAction SilentlyContinue)) {
        $Msi = Join-Path $Dist "MeshPeek-$Version-x86_64.msi"
        Run { cargo wix -p meshpeek-app --no-build --nocapture --output $Msi }
        Write-Host "    $Msi"
    } else {
        Write-Warning "WiX Toolset 3 or cargo-wix not found: skipping the MSI. Run with -InstallTools once to install them."
    }
}

Write-Host "`nDone. Packages are in $Dist" -ForegroundColor Green
