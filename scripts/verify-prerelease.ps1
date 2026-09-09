# Verifies a GitHub release of Ember against this checkout and, on request, moves it along the
# channel. -Publish turns a verified draft candidate (vX.Y.Z-rc.N) into a published prerelease.
# -Promote turns a verified stable tag (vX.Y.Z), which the build job keeps as a prerelease until
# now, into the full release that /releases/latest serves to every installed copy. Every check
# runs before any mutation. The guards are pinned by publication-guards.test.ps1 with gh and git
# mocked, so a regression here fails CI instead of the release.
param(
    [Parameter(Mandatory = $true)][string]$Tag,
    [switch]$Publish,
    [switch]$Promote,
    [string]$NotesFile
)
$ErrorActionPreference = 'Stop'
if ($Publish -and $Promote) { throw 'Choose -Publish (candidate) or -Promote (stable), not both' }
if ($Promote) {
    if ($Tag -notmatch '^v\d+\.\d+\.\d+$') { throw 'Invalid release tag' }
} elseif ($Tag -notmatch '^v\d+\.\d+\.\d+-rc\.\d+$') { throw 'Invalid candidate tag' }
if ($NotesFile -and !(Test-Path -LiteralPath $NotesFile)) { throw 'Notes file not found' }
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
if ($Tag -ne "v$version") { throw 'Tag does not match this checkout' }
$repo = 'duartelcunha/Ember'
$release = gh release view $Tag --repo $repo --json isDraft,isPrerelease,assets,targetCommitish | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect release' }
if ($Promote) {
    if ($release.isDraft) { throw 'Refusing to promote a draft: the build job has not attached its artifacts' }
    # release-please creates a stable tag as a full release and the build job flips it to a
    # prerelease before uploading. A full release here means that flip never ran (nothing is
    # verified yet) or the promotion already happened. Neither is something to act on blindly.
    if (!$release.isPrerelease) { throw "Refusing to promote ${Tag}: it is already a full release. If the build job's flip step failed, run: gh release edit $Tag --repo $repo --prerelease --latest=false, then verify and promote again" }
} else {
    if (!$release.isPrerelease) { throw 'Refusing a stable release' }
    if ($Publish -and !$release.isDraft) { throw 'Refusing to mutate a published version' }
}
if ($Publish -or $Promote) {
    $sourceCommit = git rev-parse HEAD
    if ($LASTEXITCODE -ne 0 -or $sourceCommit -notmatch '^[0-9a-f]{40}$') { throw 'Cannot establish source revision' }
    if ($release.targetCommitish -cne $sourceCommit) {
        if ($Promote) { throw 'Release belongs to a different source revision; check out the release commit before promotion' }
        throw 'Draft belongs to a different source revision; create a new candidate or repair the draft before publication'
    }
}
$directory = Join-Path (Get-Location) "target/release-verification/$Tag"
New-Item -ItemType Directory -Force -Path $directory | Out-Null
gh release download $Tag --repo $repo --dir $directory --clobber --pattern '*-setup.exe' --pattern '*.sig' --pattern latest.json
if ($LASTEXITCODE -ne 0) { throw 'Release download failed' }
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
if ($Promote) {
    gh release upload $Tag $sums --repo $repo --clobber
    if ($LASTEXITCODE -ne 0) { throw 'Checksum upload failed' }
    # One edit carries both flags: the release stops being a prerelease and becomes the one
    # /releases/latest resolves to, so the updater only ever sees a manifest whose artifacts
    # were verified above. Without -NotesFile the body written by release-please stays.
    if ($NotesFile) {
        gh release edit $Tag --repo $repo --prerelease=false --latest=true --notes-file $NotesFile
    } else {
        gh release edit $Tag --repo $repo --prerelease=false --latest=true
    }
    if ($LASTEXITCODE -ne 0) { throw 'Promotion failed' }
    Write-Output "Promoted $Tag to the stable channel"
}
Write-Output "Verified $Tag at $installer"
