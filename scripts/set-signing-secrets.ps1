# Interactively upload Apple / Windows code-signing secrets for release.yml.
# Tauri updater secrets are expected to already exist (generated once).
$ErrorActionPreference = "Stop"

function Set-SecretFromFile([string]$Name, [string]$Path) {
    if (-not (Test-Path $Path)) { throw "File not found: $Path" }
    $bytes = [System.IO.File]::ReadAllBytes((Resolve-Path $Path))
    $b64 = [Convert]::ToBase64String($bytes)
    $b64 | gh secret set $Name
    Write-Host "Set $Name from $Path"
}

Write-Host "WhimprFlow release signing secret upload"
Write-Host "Leave a prompt blank to skip that secret."

$appleP12 = Read-Host "Path to Apple Developer ID .p12 (or blank)"
if ($appleP12) {
    Set-SecretFromFile "APPLE_CERTIFICATE" $appleP12
    $p = Read-Host "APPLE_CERTIFICATE_PASSWORD"
    if ($p) { $p | gh secret set APPLE_CERTIFICATE_PASSWORD }
    $id = Read-Host "APPLE_ID (email)"
    if ($id) { $id | gh secret set APPLE_ID }
    $pw = Read-Host "APPLE_PASSWORD (app-specific password)"
    if ($pw) { $pw | gh secret set APPLE_PASSWORD }
    $team = Read-Host "APPLE_TEAM_ID"
    if ($team) { $team | gh secret set APPLE_TEAM_ID }
    $ident = Read-Host "APPLE_SIGNING_IDENTITY (Developer ID Application: ...)"
    if ($ident) { $ident | gh secret set APPLE_SIGNING_IDENTITY }
}

$winPfx = Read-Host "Path to Windows .pfx (or blank)"
if ($winPfx) {
    Set-SecretFromFile "WINDOWS_CERTIFICATE" $winPfx
    $wp = Read-Host "WINDOWS_CERTIFICATE_PASSWORD"
    if ($wp) { $wp | gh secret set WINDOWS_CERTIFICATE_PASSWORD }
}

Write-Host "Done. Current secrets:"
gh secret list
