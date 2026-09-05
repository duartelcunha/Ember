$ErrorActionPreference = 'Stop'
$global:emberTestExpectedCommit = 'a' * 40
$global:emberTestTag = 'v9.8.7-rc.1'
$verificationScript = Join-Path $PSScriptRoot 'verify-prerelease.ps1'
$global:emberTestDownloadAttempted = $false
function git { $global:LASTEXITCODE = 0; $global:emberTestExpectedCommit }
function gh {
    $global:LASTEXITCODE = 0
    if ($args[0] -eq 'release' -and $args[1] -eq 'view') { return ($global:emberTestMockRelease | ConvertTo-Json) }
    if ($args[0] -eq 'release' -and $args[1] -eq 'download') { $global:emberTestDownloadAttempted = $true; throw 'Artifact verification reached' }
    throw 'Unexpected GitHub operation'
}
function Assert-Case($draft, $commit, $expectedError, $expectedDownload) {
    $global:emberTestMockRelease = @{ isDraft=$draft; isPrerelease=$true; targetCommitish=$commit; assets=@() }
    $global:emberTestDownloadAttempted = $false
    $caught = $null
    try { & $verificationScript -Tag $global:emberTestTag -Publish } catch { $caught = $_.Exception.Message }
    if (!$caught -or !$caught.Contains($expectedError) -or $global:emberTestDownloadAttempted -ne $expectedDownload) { throw "Publication boundary failed: $caught" }
}
# Exercise the real script in an isolated candidate checkout. This also tests
# stable-version branches without changing their package metadata. All commands
# are mocked; the small fixture remains in ignored build output for inspection.
$fixtureRoot = Join-Path (Split-Path $PSScriptRoot -Parent) 'target/release-guard-fixtures'
$fixture = Join-Path $fixtureRoot ([Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
Set-Content -LiteralPath (Join-Path $fixture 'package.json') -Value '{"version":"9.8.7-rc.1"}'
Push-Location $fixture
try {
    Assert-Case $true ('0' * 40) 'Draft belongs to a different source revision' $false
    Assert-Case $false $global:emberTestExpectedCommit 'Refusing to mutate a published version' $false
    Assert-Case $true $global:emberTestExpectedCommit 'Artifact verification reached' $true
    Write-Output 'Publication guards: 3 passed; 0 failed. GitHub and git were mocked; no remote mutation.'
} finally {
    Pop-Location
}
