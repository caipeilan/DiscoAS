param(
  [Parameter(Mandatory=$true)][string]$SourcePath
)
$ErrorActionPreference = 'Stop'
# A child Windows PowerShell can inherit PowerShell 7 module search paths.
# Load the signature command from this host's own compatible module.
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1')
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1') -Force
$discoSource = (Resolve-Path -LiteralPath $SourcePath).Path
$discoSignature = Get-AuthenticodeSignature -LiteralPath $discoSource
if ($discoSignature.Status -ne 'Valid' -or $discoSignature.SignerCertificate.Subject -notmatch 'CN=Microsoft Corporation(?:,|$)') {
  throw 'The WebView2 installer must have a valid Microsoft Corporation signature.'
}
$discoResponse = Invoke-WebRequest -UseBasicParsing -Uri 'https://go.microsoft.com/fwlink/?LinkId=2124701' -Method Head -MaximumRedirection 5 -TimeoutSec 30
if ($discoResponse.BaseResponse.ResponseUri) {
  $discoFinalUri = $discoResponse.BaseResponse.ResponseUri
} else {
  $discoFinalUri = $discoResponse.BaseResponse.RequestMessage.RequestUri
}
$discoExpectedHost = 'msedge.sf.dl.delivery.mp.microsoft.com'
if ($discoFinalUri.Scheme -ne 'https' -or $discoFinalUri.Host -ne $discoExpectedHost -or
    $discoFinalUri.AbsolutePath -notmatch '^/filestreamingservice/files/([0-9a-f-]{36})/MicrosoftEdgeWebView2RuntimeInstallerX64.exe$') {
  throw 'The Microsoft download redirect did not match the expected Windows x64 installer.'
}
$discoGuid = $Matches[1]
$discoCacheRoot = Join-Path (Join-Path $env:LOCALAPPDATA 'tauri') 'x64'
$discoDestination = Join-Path (Join-Path $discoCacheRoot $discoGuid) 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe'
New-Item -ItemType Directory -Path (Split-Path -Parent $discoDestination) -Force | Out-Null
Copy-Item -LiteralPath $discoSource -Destination $discoDestination -Force
Write-Output 'WebView2 offline installer prepared.'
