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
Write-Output 'Local WebView2 offline installer ready.'
