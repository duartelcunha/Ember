# Refuse to build against a public release or a tag that points away from this source revision.
param(
    [Parameter(Mandatory = $true)][string]$Tag,
    [Parameter(Mandatory = $true)][string]$Commit,
    [Parameter(Mandatory = $true)][string]$Repository,
    [switch]$RequirePrerelease
)
$ErrorActionPreference = 'Stop'
if ($Tag -notmatch '^v\d+\.\d+\.\d+(?:-rc\.\d+)?$') { throw 'Invalid release tag' }
if ($Commit -notmatch '^[0-9a-f]{40}$') { throw 'Invalid source revision' }
if ($Repository -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') { throw 'Invalid repository' }

$release = gh release view $Tag --repo $Repository --json isDraft,isPrerelease,targetCommitish | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or !$release) { throw 'Cannot inspect release' }
if (!$release.isDraft) { throw 'Refusing a release that is already public' }
if ($RequirePrerelease -and !$release.isPrerelease) { throw 'Release is not a prerelease' }
if ($release.targetCommitish -cne $Commit) { throw 'Release target differs from source revision' }

$reference = gh api "repos/$Repository/git/ref/tags/$Tag" | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or !$reference) { throw 'Cannot inspect release tag' }
if ($reference.object.type -cne 'commit' -or $reference.object.sha -cne $Commit) {
    throw 'Release tag differs from source revision'
}
Write-Output "Draft release $Tag points to the expected source revision"
