$ErrorActionPreference = 'Stop'
$global:emberTestExpectedCommit = 'a' * 40
$global:emberTestTag = 'v9.8.7-rc.1'
$global:emberTestStableTag = 'v9.8.7'
$verificationScript = Join-Path $PSScriptRoot 'verify-prerelease.ps1'
$global:emberTestDownloadAttempted = $false
$global:emberTestCases = 0
function git { $global:LASTEXITCODE = 0; $global:emberTestExpectedCommit }
function gh {
    $global:LASTEXITCODE = 0
    if ($args[0] -eq 'release' -and $args[1] -eq 'view') { return ($global:emberTestMockRelease | ConvertTo-Json) }
    if ($args[0] -eq 'release' -and $args[1] -eq 'download') { $global:emberTestDownloadAttempted = $true; throw 'Artifact verification reached' }
    throw 'Unexpected GitHub operation'
}
function Assert-Boundary($release, $arguments, $expectedError, $expectedDownload) {
    $global:emberTestMockRelease = $release
    $global:emberTestDownloadAttempted = $false
    $caught = $null
    try { & $verificationScript @arguments } catch { $caught = $_.Exception.Message }
    if (!$caught -or !$caught.Contains($expectedError) -or $global:emberTestDownloadAttempted -ne $expectedDownload) { throw "Publication boundary failed: $caught" }
    $global:emberTestCases++
}
function Assert-Case($draft, $commit, $expectedError, $expectedDownload) {
    $release = @{ isDraft=$draft; isPrerelease=$true; targetCommitish=$commit; assets=@() }
    Assert-Boundary $release @{ Tag=$global:emberTestTag; Publish=$true } $expectedError $expectedDownload
}
function Assert-Promote($draft, $prerelease, $commit, $tag, $expectedError, $expectedDownload) {
    $release = @{ isDraft=$draft; isPrerelease=$prerelease; targetCommitish=$commit; assets=@() }
    Assert-Boundary $release @{ Tag=$tag; Promote=$true } $expectedError $expectedDownload
}
function New-Fixture($version) {
    $fixture = Join-Path $fixtureRoot ([Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $fixture | Out-Null
    Set-Content -LiteralPath (Join-Path $fixture 'package.json') -Value ('{"version":"' + $version + '"}')
    $fixture
}
# Exercise the real script in isolated checkouts: one candidate (9.8.7-rc.1) and one stable
# (9.8.7), without changing this repository's package metadata. All commands are mocked; the
# small fixtures remain in ignored build output for inspection.
$fixtureRoot = Join-Path (Split-Path $PSScriptRoot -Parent) 'target/release-guard-fixtures'
Push-Location (New-Fixture '9.8.7-rc.1')
try {
    Assert-Case $true ('0' * 40) 'Draft belongs to a different source revision' $false
    Assert-Case $false $global:emberTestExpectedCommit 'Refusing to mutate a published version' $false
    Assert-Case $true $global:emberTestExpectedCommit 'Artifact verification reached' $true
    # Promotion never accepts a candidate tag, whatever the release looks like.
    Assert-Promote $false $true $global:emberTestExpectedCommit $global:emberTestTag 'Invalid release tag' $false
} finally {
    Pop-Location
}
Push-Location (New-Fixture '9.8.7')
try {
    # Both switches at once is a contradiction, not a combination.
    Assert-Boundary @{} @{ Tag=$global:emberTestStableTag; Publish=$true; Promote=$true } 'not both' $false
    # A stable tag is born as a full release and the build job flips it to a prerelease before
    # uploading. Seeing a full release here means that flip never ran or the promotion already
    # happened; neither is something to act on blindly.
    Assert-Promote $false $false $global:emberTestExpectedCommit $global:emberTestStableTag 'already a full release' $false
    Assert-Promote $true $true $global:emberTestExpectedCommit $global:emberTestStableTag 'Refusing to promote a draft' $false
    Assert-Promote $false $true ('0' * 40) $global:emberTestStableTag 'Release belongs to a different source revision' $false
    Assert-Promote $false $true $global:emberTestExpectedCommit $global:emberTestStableTag 'Artifact verification reached' $true
} finally {
    Pop-Location
}
Write-Output "Publication guards: $global:emberTestCases passed; 0 failed. GitHub and git were mocked; no remote mutation."
