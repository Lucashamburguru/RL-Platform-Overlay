[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$PackageName,

    [Parameter(Mandatory = $true)]
    [string]$Publisher,

    [Parameter(Mandatory = $true)]
    [string]$PublisherDisplayName,

    [string]$DisplayName = "RL Platform Overlay",
    [string]$Version,
    [string]$CertificatePath,
    [string]$CertificatePassword,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$cargoToml = Join-Path $repoRoot "Cargo.toml"
$manifestTemplate = Join-Path $PSScriptRoot "AppxManifest.xml.in"
$assetsSource = Join-Path $PSScriptRoot "Assets"
$outputRoot = Join-Path $repoRoot "target\msix"
$stagingRoot = Join-Path $outputRoot "staging"
$targetTriple = "x86_64-pc-windows-msvc"

function Get-CargoVersion {
    $content = Get-Content -LiteralPath $cargoToml -Raw
    $match = [regex]::Match($content, '(?m)^version\s*=\s*"(?<version>\d+\.\d+\.\d+)"')
    if (-not $match.Success) {
        throw "Could not read the package version from Cargo.toml."
    }
    return [version]$match.Groups["version"].Value
}

function Get-StoreVersion([version]$CargoVersion) {
    # Store package versions cannot begin with zero and reserve the fourth field.
    return "{0}.{1}.{2}.0" -f ($CargoVersion.Major + 1), $CargoVersion.Minor, $CargoVersion.Build
}

function Escape-Xml([string]$Value) {
    return [System.Security.SecurityElement]::Escape($Value)
}

function Find-WindowsSdkTool([string]$Name) {
    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    $sdkBin = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
    if (-not (Test-Path -LiteralPath $sdkBin)) {
        throw "$Name was not found. Install the Windows SDK signing tools."
    }

    $tool = Get-ChildItem -LiteralPath $sdkBin -Filter $Name -File -Recurse |
        Where-Object { $_.DirectoryName -match '\\x64$' } |
        Sort-Object FullName -Descending |
        Select-Object -First 1
    if (-not $tool) {
        throw "$Name was not found beneath $sdkBin."
    }
    return $tool.FullName
}

$cargoVersion = Get-CargoVersion
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = Get-StoreVersion $cargoVersion
}
if ($Version -notmatch '^[1-9]\d{0,4}\.\d{1,5}\.\d{1,5}\.0$') {
    throw "MSIX version '$Version' must contain four numeric fields, begin above zero, and end in .0."
}

$requiredAssets = @(
    "StoreLogo.png",
    "Square44x44Logo.png",
    "Square150x150Logo.png",
    "Wide310x150Logo.png",
    "Square310x310Logo.png"
)
foreach ($asset in $requiredAssets) {
    $assetPath = Join-Path $assetsSource $asset
    if (-not (Test-Path -LiteralPath $assetPath)) {
        throw "Missing MSIX visual asset: $assetPath"
    }
}

if (-not $SkipBuild) {
    Push-Location $repoRoot
    try {
        & cargo build --locked --release --target $targetTriple --features microsoft-store
        if ($LASTEXITCODE -ne 0) {
            throw "The Microsoft Store Rust build failed."
        }
    }
    finally {
        Pop-Location
    }
}

$executable = Join-Path $repoRoot "target\$targetTriple\release\rl-platform-overlay.exe"
if (-not (Test-Path -LiteralPath $executable)) {
    throw "Store executable not found at $executable."
}

if (Test-Path -LiteralPath $stagingRoot) {
    Remove-Item -LiteralPath $stagingRoot -Recurse -Force
}
New-Item -ItemType Directory -Path (Join-Path $stagingRoot "Assets") -Force | Out-Null
Copy-Item -LiteralPath $executable -Destination $stagingRoot
foreach ($asset in $requiredAssets) {
    Copy-Item -LiteralPath (Join-Path $assetsSource $asset) -Destination (Join-Path $stagingRoot "Assets")
}

$manifest = Get-Content -LiteralPath $manifestTemplate -Raw
$replacements = @{
    "@@PACKAGE_NAME@@" = Escape-Xml $PackageName
    "@@PUBLISHER@@" = Escape-Xml $Publisher
    "@@PUBLISHER_DISPLAY_NAME@@" = Escape-Xml $PublisherDisplayName
    "@@DISPLAY_NAME@@" = Escape-Xml $DisplayName
    "@@VERSION@@" = $Version
}
foreach ($token in $replacements.Keys) {
    $manifest = $manifest.Replace($token, $replacements[$token])
}
Set-Content -LiteralPath (Join-Path $stagingRoot "AppxManifest.xml") -Value $manifest -Encoding utf8

$safeName = $PackageName -replace '[^A-Za-z0-9._-]', '_'
$packagePath = Join-Path $outputRoot ("{0}_{1}_x64.msix" -f $safeName, $Version)
$makeAppx = Find-WindowsSdkTool "MakeAppx.exe"
& $makeAppx pack /d $stagingRoot /p $packagePath /o
if ($LASTEXITCODE -ne 0) {
    throw "MakeAppx failed."
}

if (-not [string]::IsNullOrWhiteSpace($CertificatePath)) {
    $signTool = Find-WindowsSdkTool "SignTool.exe"
    $signArguments = @("sign", "/fd", "SHA256", "/f", $CertificatePath)
    if (-not [string]::IsNullOrWhiteSpace($CertificatePassword)) {
        $signArguments += @("/p", $CertificatePassword)
    }
    $signArguments += $packagePath
    & $signTool @signArguments
    if ($LASTEXITCODE -ne 0) {
        throw "SignTool failed."
    }
}

Write-Host "Created $packagePath"
