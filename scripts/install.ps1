[CmdletBinding()]
param([switch] $AddToPath)

$ErrorActionPreference = 'Stop'

$architecture = $env:PROCESSOR_ARCHITEW6432
if (!$architecture) {
    $architecture = $env:PROCESSOR_ARCHITECTURE
}
if ($architecture -ne 'AMD64') {
    throw "Unsupported Windows architecture: $architecture"
}
$archive = 'my-skills-x86_64-pc-windows-msvc.zip'

$releases = 'https://github.com/Seosiju/my-skills/releases'
$version = $env:MY_SKILLS_VERSION
if ($version -match '[^a-zA-Z0-9.-]' -or $version -in '.', '..') {
    throw 'MY_SKILLS_VERSION must be a release tag'
}
if ($env:MY_SKILLS_RELEASE_BASE_URL) {
    $baseUrl = $env:MY_SKILLS_RELEASE_BASE_URL -replace '/$', ''
} elseif ($version) {
    $baseUrl = "$releases/download/$version"
} else {
    $baseUrl = "$releases/latest/download"
}

$installDir = $env:MY_SKILLS_INSTALL_DIR
if (!$installDir) {
    $installDir = Join-Path $env:LOCALAPPDATA 'Programs\my-skills\bin'
}
$installDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($installDir)
$destination = Join-Path $installDir 'my-skills.exe'

function Get-ReleaseAsset([string] $Name) {
    $uri = [uri] "$baseUrl/$Name"
    $outputPath = Join-Path $workDir $Name
    if ($uri.IsFile) {
        Copy-Item -LiteralPath $uri.LocalPath -Destination $outputPath
    } else {
        Invoke-WebRequest -UseBasicParsing -Uri $uri -OutFile $outputPath
    }
}

function Add-PathEntry([string] $PathValue, [string] $Directory) {
    $separators = [char[]] '\/'
    $entries = $PathValue -split ';' | ForEach-Object { $_.Trim('"').TrimEnd($separators) }
    if ($entries -contains $Directory.TrimEnd($separators)) {
        return $PathValue
    }
    if (!$PathValue) {
        return $Directory
    }
    return ($PathValue.TrimEnd(';') + ';' + $Directory)
}

$workDir = Join-Path ([IO.Path]::GetTempPath()) "my-skills-$([guid]::NewGuid())"
$stagedPath = $null
New-Item -ItemType Directory -Path $workDir | Out-Null
try {
    Get-ReleaseAsset 'SHA256SUMS'
    $pattern = '^([^\s]+)\s+\*?' + [regex]::Escape($archive) + '$'
    $checksums = @(Select-String -LiteralPath (Join-Path $workDir 'SHA256SUMS') -Pattern $pattern -CaseSensitive)
    if ($checksums.Count -ne 1 -or $checksums[0].Matches[0].Groups[1].Value -notmatch '^[0-9a-f]{64}$') {
        throw "Missing or invalid checksum for $archive"
    }
    $expected = $checksums[0].Matches[0].Groups[1].Value

    Get-ReleaseAsset $archive
    $archivePath = Join-Path $workDir $archive
    $actual = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash
    if ($actual -ne $expected) {
        throw "SHA-256 mismatch for $archive"
    }

    $extractDir = Join-Path $workDir 'extract'
    Expand-Archive -LiteralPath $archivePath -DestinationPath $extractDir
    $binary = Join-Path $extractDir 'my-skills.exe'
    if (!(Test-Path -LiteralPath $binary -PathType Leaf) -or (Get-Item -LiteralPath $binary).Length -eq 0) {
        throw "Missing binary in $archive"
    }

    [IO.Directory]::CreateDirectory($installDir) | Out-Null
    $stagedPath = Join-Path $installDir ".my-skills-$([guid]::NewGuid()).tmp"
    Copy-Item -LiteralPath $binary -Destination $stagedPath
    if ([IO.File]::Exists($destination)) {
        [IO.File]::Replace($stagedPath, $destination, [NullString]::Value)
    } else {
        [IO.File]::Move($stagedPath, $destination)
    }
} finally {
    if ($stagedPath -and (Test-Path -LiteralPath $stagedPath)) {
        Remove-Item -LiteralPath $stagedPath -Force
    }
    Remove-Item -LiteralPath $workDir -Recurse -Force
}

Write-Output "Installed $destination"
$sessionPath = Add-PathEntry $env:Path $installDir
if ($AddToPath) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $newUserPath = Add-PathEntry $userPath $installDir
    if ($newUserPath -ne $userPath) {
        [Environment]::SetEnvironmentVariable('Path', $newUserPath, 'User')
    }
    $env:Path = $sessionPath
} elseif ($sessionPath -ne $env:Path) {
    Write-Output "Add $installDir to PATH to run my-skills from any directory. Rerun with -AddToPath to add it to your user PATH."
}
Write-Output ''
Write-Output @'
Next:
  New here?            my-skills init-registry
  Registry elsewhere?  git clone <your-private-repo> "$HOME/my-agent-skills"
                       cd "$HOME/my-agent-skills"
                       my-skills bootstrap
'@
