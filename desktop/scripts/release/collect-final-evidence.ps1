[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ReleaseBatchId,
    [Parameter(Mandatory = $true)][string]$CandidatePath,
    [Parameter(Mandatory = $true)][string]$MetadataInput
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Resolve-ExistingFile([string]$Value, [string]$Name) {
    $item = Get-Item -LiteralPath $Value -Force
    if (-not $item -or $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "$Name must be an existing non-link file"
    }
    return $item.FullName
}

function Test-RelatedPath([string]$Left, [string]$Right) {
    $leftFull = [IO.Path]::GetFullPath($Left).TrimEnd('\')
    $rightFull = [IO.Path]::GetFullPath($Right).TrimEnd('\')
    return $leftFull.Equals($rightFull, [StringComparison]::OrdinalIgnoreCase) -or
        $leftFull.StartsWith($rightFull + '\', [StringComparison]::OrdinalIgnoreCase) -or
        $rightFull.StartsWith($leftFull + '\', [StringComparison]::OrdinalIgnoreCase)
}

function Select-EvidenceRoot([string[]]$ProtectedPaths) {
    $shell = New-Object -ComObject Shell.Application
    try {
        $selection = $shell.BrowseForFolder(0, 'Select a dedicated final-release evidence directory', 0x1, 0)
        if (-not $selection) { throw 'Evidence directory selection was cancelled' }
        $full = [IO.Path]::GetFullPath($selection.Self.Path)
    } finally {
        if ($shell) { [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell) | Out-Null }
    }
    $item = Get-Item -LiteralPath $full -Force
    if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'Evidence root must be an existing non-link directory selected by the native picker'
    }
    if ([IO.Path]::GetPathRoot($full).TrimEnd('\') -eq $full.TrimEnd('\')) {
        throw 'Evidence root must not be a volume root'
    }
    $cursor = $item
    while ($cursor) {
        if ($cursor.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw 'Evidence root must not traverse a reparse point or junction'
        }
        $cursor = $cursor.Parent
    }
    foreach ($protected in $ProtectedPaths) {
        if ($protected -and (Test-RelatedPath $full $protected)) {
            throw 'Evidence root must be isolated from profile, application data, source, candidate, and metadata roots'
        }
    }
    return $full
}

function Test-SafeMetadata($Value, [int]$Depth, [ref]$NodeCount, $ForbiddenNames) {
    if ($Depth -gt 12) { throw 'MetadataInput exceeds maximum depth' }
    $NodeCount.Value++
    if ($NodeCount.Value -gt 5000) { throw 'MetadataInput exceeds maximum node count' }
    if ($null -eq $Value) { return }
    if ($Value -is [string]) {
        if ([Text.Encoding]::UTF8.GetByteCount($Value) -gt 8192) { throw 'MetadataInput string is too large' }
        return
    }
    if ($Value -is [System.Collections.IDictionary]) {
        foreach ($key in $Value.Keys) {
            if ($ForbiddenNames.Contains([string]$key)) { throw "MetadataInput contains forbidden field: $key" }
            Test-SafeMetadata $Value[$key] ($Depth + 1) $NodeCount $ForbiddenNames
        }
        return
    }
    if ($Value -is [System.Collections.IEnumerable] -and -not ($Value -is [pscustomobject])) {
        foreach ($item in $Value) { Test-SafeMetadata $item ($Depth + 1) $NodeCount $ForbiddenNames }
        return
    }
    if ($Value -is [pscustomobject]) {
        foreach ($property in $Value.PSObject.Properties) {
            if ($ForbiddenNames.Contains($property.Name)) { throw "MetadataInput contains forbidden field: $($property.Name)" }
            Test-SafeMetadata $property.Value ($Depth + 1) $NodeCount $ForbiddenNames
        }
    }
}

if ($ReleaseBatchId -notmatch '^[A-Za-z0-9][A-Za-z0-9_-]{7,63}$') {
    throw 'ReleaseBatchId must be an 8-64 character opaque identifier'
}
$candidate = Resolve-ExistingFile $CandidatePath 'CandidatePath'
$metadata = Resolve-ExistingFile $MetadataInput 'MetadataInput'
$metadataBytes = [IO.File]::ReadAllBytes($metadata)
if ($metadataBytes.Length -gt 262144) { throw 'MetadataInput exceeds 256 KiB' }
$metadataValue = [Text.Encoding]::UTF8.GetString($metadataBytes) | ConvertFrom-Json
$forbidden = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
@('prompt', 'body', 'apiKey', 'authorization', 'cookie', 'sourcePath', 'messageText', 'ocrText', 'replyText') |
    ForEach-Object { [void]$forbidden.Add($_) }
$nodeCount = 0
Test-SafeMetadata $metadataValue 0 ([ref]$nodeCount) $forbidden

$protected = @(
    [Environment]::GetFolderPath('UserProfile'),
    [Environment]::GetFolderPath('LocalApplicationData'),
    [Environment]::GetFolderPath('ApplicationData'),
    [Environment]::GetFolderPath('CommonApplicationData'),
    [IO.Path]::GetTempPath(),
    (Get-Location).Path,
    [IO.Path]::GetDirectoryName($candidate),
    [IO.Path]::GetDirectoryName($metadata)
)
$root = Select-EvidenceRoot $protected
$batchRoot = [IO.Path]::GetFullPath((Join-Path $root $ReleaseBatchId))
if (-not $batchRoot.StartsWith($root.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Release batch path escaped the selected evidence root'
}
if (Test-Path -LiteralPath $batchRoot) {
    throw 'The batch evidence directory already exists; evidence is append-by-new-batch only'
}

$signature = Get-AuthenticodeSignature -LiteralPath $candidate
$record = [ordered]@{
    schemaVersion = 1
    releaseBatchId = $ReleaseBatchId
    capturedAt = [DateTimeOffset]::UtcNow.ToString('o')
    machine = [ordered]@{
        os = [Environment]::OSVersion.VersionString
        architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
        powershell = $PSVersionTable.PSVersion.ToString()
    }
    candidate = [ordered]@{
        fileName = [IO.Path]::GetFileName($candidate)
        sha256 = (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash.ToLowerInvariant()
        bytes = (Get-Item -LiteralPath $candidate).Length
    }
    authenticode = [ordered]@{
        status = $signature.Status.ToString()
        signerThumbprint = if ($signature.SignerCertificate) { $signature.SignerCertificate.Thumbprint } else { $null }
        timestampThumbprint = if ($signature.TimeStamperCertificate) { $signature.TimeStamperCertificate.Thumbprint } else { $null }
    }
    metadata = $metadataValue
}

$pending = Join-Path $root ('.pending-' + [Guid]::NewGuid().ToString('N'))
try {
    New-Item -ItemType Directory -Path $pending | Out-Null
    $output = Join-Path $pending 'windows-collector.json'
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($record | ConvertTo-Json -Depth 20))
    $stream = [IO.File]::Open($output, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
    } finally {
        $stream.Dispose()
    }
    Move-Item -LiteralPath $pending -Destination $batchRoot
} catch {
    if (Test-Path -LiteralPath $pending) { Remove-Item -LiteralPath $pending -Recurse -Force }
    throw
}
Write-Output (Join-Path $batchRoot 'windows-collector.json')
