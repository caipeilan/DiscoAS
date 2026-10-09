param([Parameter(Mandatory=$true)][string]$Archive,
      [Parameter(Mandatory=$true)][string]$Destination)
$ErrorActionPreference = 'Stop'
# The calling Rust service verifies the pinned SHA-256 before invoking this file.
# Both paths are separate arguments, never shell fragments. Only the application
# owned, new tool directory is extracted; existing Spicetify is never overwritten.
if (Test-Path -LiteralPath $Destination) {
    throw 'Destination must be a new directory'
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::ExtractToDirectory($Archive, $Destination)
if (-not (Test-Path -LiteralPath (Join-Path $Destination 'spicetify.exe') -PathType Leaf)) {
    throw 'Spicetify archive is incomplete'
}
