param(
    [Parameter(Mandatory = $true)][string]$Tag,
    [switch]$Publish
)
$ErrorActionPreference = 'Stop'
if ($Tag -notmatch '^v\d+\.\d+\.\d+-rc\.\d+$') { throw 'Invalid candidate tag' }
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
if ($Tag -ne "v$version") { throw 'Candidate does not match this checkout' }
$repo = 'duartelcunha/Ember'
$release = gh release view $Tag --repo $repo --json isDraft,isPrerelease,assets,targetCommitish | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect candidate release' }
if (!$release.isPrerelease) { throw 'Refusing a stable release' }
if ($Publish -and !$release.isDraft) { throw 'Refusing to mutate a published version' }
if ($Publish) {
    $sourceCommit = git rev-parse HEAD
    if ($LASTEXITCODE -ne 0 -or $sourceCommit -notmatch '^[0-9a-f]{40}$') { throw 'Cannot establish source revision' }
    if ($release.targetCommitish -cne $sourceCommit) { throw 'Draft belongs to a different source revision; create a new candidate or repair the draft before publication' }
}
$directory = Join-Path (Get-Location) "target/release-verification/$Tag"
New-Item -ItemType Directory -Force -Path $directory | Out-Null
gh release download $Tag --repo $repo --dir $directory --clobber --pattern '*-setup.exe' --pattern '*.sig' --pattern latest.json
if ($LASTEXITCODE -ne 0) { throw 'Candidate download failed' }
$manifest = Get-Content (Join-Path $directory 'latest.json') -Raw | ConvertFrom-Json
if ($manifest.version.TrimStart('v') -ne $version) { throw 'Updater version mismatch' }
$platform = $manifest.platforms.'windows-x86_64'
if (!$platform) { throw 'Missing Windows updater target' }
$installerName = "Ember_${version}_x64-setup.exe"
$expectedUrl = "https://github.com/$repo/releases/download/$Tag/$installerName"
if ($platform.url -cne $expectedUrl) { throw 'Updater installer URL mismatch' }
$installer = Join-Path $directory $installerName
$signature = "$installer.sig"
if ($platform.signature.Trim() -cne (Get-Content $signature -Raw).Trim()) { throw 'Updater signature mismatch' }
cargo run --locked -p ember --example verify_update -- $installer $signature
if ($LASTEXITCODE -ne 0) { throw 'Updater signature verification failed' }
$hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
$sums = Join-Path $directory 'SHA256SUMS.txt'
Set-Content -LiteralPath $sums -Encoding utf8NoBOM -Value "$hash  $installerName"
if ($Publish) {
    gh release upload $Tag $sums --repo $repo --clobber
    if ($LASTEXITCODE -ne 0) { throw 'Checksum upload failed' }
    gh release edit $Tag --repo $repo --draft=false --prerelease --latest=false --notes-file docs/prerelease-notes.md
    if ($LASTEXITCODE -ne 0) { throw 'Candidate publication failed' }
}
Write-Output "Verified candidate $Tag at $installer"
