$ErrorActionPreference = 'Stop'
$script = Join-Path $PSScriptRoot 'assert-draft-release.ps1'
$commit = 'a' * 40
$other = 'b' * 40
$tag = 'v9.8.7-rc.1'
$repository = 'example/ember'
$global:mockRelease = @{}
$global:mockTag = @{}
$global:mockApiExit = 0
$cases = 0
function gh {
    if ($args[0] -eq 'release' -and $args[1] -eq 'view') {
        $global:LASTEXITCODE = 0
        return ($global:mockRelease | ConvertTo-Json)
    }
    if ($args[0] -eq 'api') {
        $global:LASTEXITCODE = $global:mockApiExit
        if ($global:mockApiExit -eq 0) { return ($global:mockTag | ConvertTo-Json -Depth 3) }
        return
    }
    throw 'Unexpected GitHub operation'
}
function Assert-Case($release, $tagReference, $apiExit, $requirePrerelease, $expectedError) {
    $global:mockRelease = $release
    $global:mockTag = $tagReference
    $global:mockApiExit = $apiExit
    $caught = $null
    try {
        & $script -Tag $tag -Commit $commit -Repository $repository -RequirePrerelease:$requirePrerelease | Out-Null
    } catch { $caught = $_.Exception.Message }
    if ($expectedError) {
        if (!$caught -or !$caught.Contains($expectedError)) { throw "Draft guard expected '$expectedError', got '$caught'" }
    } elseif ($caught) { throw "Draft guard unexpectedly failed: $caught" }
    $script:cases++
}

$draft = @{ isDraft=$true; isPrerelease=$true; targetCommitish=$commit }
$correctTag = @{ object=@{ type='commit'; sha=$commit } }
Assert-Case $draft $correctTag 0 $true $null
Assert-Case @{ isDraft=$false; isPrerelease=$true; targetCommitish=$commit } $correctTag 0 $true 'already public'
Assert-Case @{ isDraft=$true; isPrerelease=$false; targetCommitish=$commit } $correctTag 0 $true 'not a prerelease'
Assert-Case @{ isDraft=$true; isPrerelease=$true; targetCommitish=$other } $correctTag 0 $true 'target differs'
Assert-Case $draft @{ object=@{ type='commit'; sha=$other } } 0 $true 'tag differs'
Assert-Case $draft $correctTag 1 $true 'Cannot inspect release tag'
Write-Output "Draft release guards: $cases passed; 0 failed. GitHub was mocked."
