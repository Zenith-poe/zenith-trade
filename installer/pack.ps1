# Builds the release exe and packs it with Velopack: Setup.exe plus the update packages in
# target\releases. Needs the .NET SDK and `dotnet tool install -g vpk`. Run from anywhere.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    $version = (Select-String -Path 'Cargo.toml' -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
    $stage = 'target\pack-stage'
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    New-Item -ItemType Directory $stage | Out-Null
    Copy-Item 'target\release\zenith.exe' $stage
    vpk pack --packId Zenith --packVersion $version --packDir $stage --mainExe zenith.exe `
        --icon 'crates\app\assets\zenith.ico' --packTitle "Zenith Trade" --splashImage 'installer/splash.png' --outputDir 'target\releases'
    if ($LASTEXITCODE -ne 0) { throw 'vpk pack failed' }
    Get-ChildItem 'target\releases' | Select-Object Name, Length
} finally { Pop-Location }
