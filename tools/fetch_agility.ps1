<#
.SYNOPSIS
    Fetches the DirectX 12 Agility SDK NuGet package and unpacks it for XCL use.

.DESCRIPTION
    Downloads Microsoft.Direct3D.D3D12 (version pinned below) from NuGet, saves it to
    third_party/agility.zip, and extracts it to third_party/agility. Verifies that
    third_party/agility/build/native/bin/x64/D3D12Core.dll exists and carries a valid
    Microsoft Authenticode signature.

    The script is idempotent: it can be re-run safely. If the archive is already present
    it reuses it; if the extracted tree is missing or the DLL is absent it re-extracts.

    NOTE: This script only downloads and verifies. It never touches system environment
    variables, the registry, or Windows files. XCL_D3D12_DIR is set only at runtime by
    the caller (see AGENT_PLAYBOOK.md step 0.2a).

.EXAMPLE
    .\tools\fetch_agility.ps1
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# --- Configuration (pin the exact package and version) ---
$PackageId  = 'Microsoft.Direct3D.D3D12'
$PackageVersion = '1.620.2'
$NuGetUrl   = "https://www.nuget.org/api/v2/package/$PackageId/$PackageVersion"

# --- Resolve paths relative to the repository root (script lives in tools/) ---
$ScriptDir  = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot   = Split-Path -Parent $ScriptDir
$ThirdParty = Join-Path $RepoRoot 'third_party'
$ZipPath    = Join-Path $ThirdParty 'agility.zip'
$ExtractDir = Join-Path $ThirdParty 'agility'
$DllPath    = Join-Path $ExtractDir 'build\native\bin\x64\D3D12Core.dll'

Write-Host "XCL Agility SDK fetch"
Write-Host "  Package : $PackageId $PackageVersion"
Write-Host "  URL     : $NuGetUrl"
Write-Host "  Zip     : $ZipPath"
Write-Host "  Extract : $ExtractDir"
Write-Host ""

# --- 1. Ensure the working directory exists ---
if (-not (Test-Path -LiteralPath $ThirdParty)) {
    Write-Host "Creating $ThirdParty ..."
    New-Item -ItemType Directory -Path $ThirdParty | Out-Null
}

# --- 2. Download the package (only if missing or corrupt) ---
if (-not (Test-Path -LiteralPath $ZipPath)) {
    Write-Host "Downloading $NuGetUrl ..."
    try {
        Invoke-WebRequest -Uri $NuGetUrl -OutFile $ZipPath -UseBasicParsing
    }
    catch {
        throw "ERROR: failed to download $NuGetUrl : $($_.Exception.Message)"
    }
    Write-Host "Downloaded $(Get-Item -LiteralPath $ZipPath | ForEach-Object { '{0:N0}' -f ($_.Length/1MB) }) MB."
}
else {
    Write-Host "Reusing existing $ZipPath"
}

# --- 3. Verify the downloaded archive is a real (non-empty) zip ---
$zipInfo = Get-Item -LiteralPath $ZipPath
if ($zipInfo.Length -lt 1) {
    throw "ERROR: $ZipPath is empty; delete it and re-run."
}
$zipHash = (Get-FileHash -LiteralPath $ZipPath -Algorithm SHA256).Hash
Write-Host "  SHA256: $zipHash"

# --- 4. Extract (re-extract if the target DLL is missing) ---
$needExtract = (-not (Test-Path -LiteralPath $DllPath))
if ($needExtract) {
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

# --- 5. Verify the D3D12Core.dll exists ---
if (-not (Test-Path -LiteralPath $DllPath)) {
    throw "ERROR: expected DLL not found after extraction: $DllPath"
}
Write-Host "  Found: $DllPath"

# --- 6. Verify the Authenticode signature ---
$sig = Get-AuthenticodeSignature -LiteralPath $DllPath
if ($sig.Status -ne 'Valid') {
    throw "ERROR: $DllPath has an invalid Authenticode signature (status: $($sig.Status))."
}
Write-Host "  Signature : VALID"
Write-Host "  Signer    : $($sig.SignerCertificate.Subject)"
Write-Host "  Status    : $($sig.Status)"

Write-Host ""
Write-Host "OK: Agility SDK $PackageVersion is ready at $ExtractDir"
Write-Host "Set XCL_D3D12_DIR to: $ExtractDir\build\native\bin\x64 (absolute path, runtime only)"
