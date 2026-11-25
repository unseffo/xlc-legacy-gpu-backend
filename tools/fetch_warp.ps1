<#
.SYNOPSIS
    Fetches the Microsoft.Direct3D.WARP (Windows Advanced Rasterization Platform)
    NuGet package and unpacks it for XCL use.

.DESCRIPTION
    Downloads Microsoft.Direct3D.WARP (version pinned below) from NuGet into
    third_party/warp.zip and extracts it to third_party/warp. The package ships a
    redistributable d3d10warp.dll: copied next to an exe (the DLL search order
    puts the application directory before System32) the app uses this WARP
    instead of the OS one - the point is a mesh-shader-capable WARP reference
    renderer on the single-GPU (GTX 1080) machine.

    Idempotent: re-runs reuse the archive and re-extract only when the DLL is
    missing. Verifies the Authenticode signature and prints the file version.
    Nothing is committed (third_party/ is gitignored) and the system is not
    modified: the DLL is only ever copied next to a program by the caller.

.EXAMPLE
    .\tools\fetch_warp.ps1
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$PackageId      = 'Microsoft.Direct3D.WARP'
$PackageVersion = '1.0.22'
$NuGetUrl       = "https://www.nuget.org/api/v2/package/$PackageId/$PackageVersion"

$ScriptDir  = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot   = Split-Path -Parent $ScriptDir
$ThirdParty = Join-Path $RepoRoot 'third_party'
$ZipPath    = Join-Path $ThirdParty 'warp.zip'
$ExtractDir = Join-Path $ThirdParty 'warp'

Write-Host "XCL WARP fetch"
Write-Host "  Package : $PackageId $PackageVersion"
Write-Host "  URL     : $NuGetUrl"
Write-Host "  Zip     : $ZipPath"
Write-Host "  Extract : $ExtractDir"
Write-Host ""

if (-not (Test-Path -LiteralPath $ThirdParty)) {
    New-Item -ItemType Directory -Path $ThirdParty | Out-Null
}

if (-not (Test-Path -LiteralPath $ZipPath)) {
    Write-Host "Downloading $NuGetUrl ..."
    try {
        Invoke-WebRequest -Uri $NuGetUrl -OutFile $ZipPath -UseBasicParsing
    }
    catch {
        throw "ERROR: failed to download $NuGetUrl : $($_.Exception.Message)"
    }
}
else {
    Write-Host "Reusing existing $ZipPath"
}

$zipInfo = Get-Item -LiteralPath $ZipPath
if ($zipInfo.Length -lt 1) {
    throw "ERROR: $ZipPath is empty; delete it and re-run."
}
$zipHash = (Get-FileHash -LiteralPath $ZipPath -Algorithm SHA256).Hash
Write-Host "  SHA256: $zipHash"

$anyDll = @(Get-ChildItem -LiteralPath $ExtractDir -Recurse -Filter d3d10warp.dll -ErrorAction SilentlyContinue)
if ($anyDll.Count -eq 0) {
    if (Test-Path -LiteralPath $ExtractDir) {
        Write-Host "Removing stale $ExtractDir ..."
        Remove-Item -LiteralPath $ExtractDir -Recurse -Force
    }
    Write-Host "Extracting to $ExtractDir ..."
    try {
        Expand-Archive -LiteralPath $ZipPath -DestinationPath $ExtractDir -Force
    }
    catch {
        throw "ERROR: failed to extract $ZipPath : $($_.Exception.Message)"
    }
}
else {
    Write-Host "Extracted tree already present; skipping extraction."
}

$dlls = @(Get-ChildItem -LiteralPath $ExtractDir -Recurse -Filter d3d10warp.dll)
if ($dlls.Count -eq 0) {
    throw "ERROR: no d3d10warp.dll found under $ExtractDir"
}
Write-Host ""
foreach ($d in $dlls) {
    $rel = $d.FullName.Substring($ExtractDir.Length + 1)
    $arch = if ($d.FullName -match '\\(x64|amd64|arm64)\\') { $Matches[1] } else { '?' }
    Write-Host ("  {0}  [{1}]  {2:N0} bytes" -f $rel, $arch, $d.Length)
}
$x64 = @($dlls | Where-Object { $_.FullName -match '\\(x64|amd64)\\' })
if ($x64.Count -gt 0) {
    $dll = $x64[0]
    $sig = Get-AuthenticodeSignature -LiteralPath $dll.FullName
    $ver = $dll.VersionInfo.FileVersion
    if ($sig.Status -ne 'Valid') {
        throw "ERROR: $($dll.FullName) has an invalid Authenticode signature (status: $($sig.Status))."
    }
    Write-Host ""
    Write-Host "OK: x64 d3d10warp.dll $ver, signature VALID ($($sig.SignerCertificate.Subject))"
    Write-Host "Copy it next to the program that should use this WARP (DLL search order puts the exe dir before System32)."
}
else {
    throw "ERROR: no x64/amd64 d3d10warp.dll found under $ExtractDir"
}
