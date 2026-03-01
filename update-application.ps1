<#
.SYNOPSIS
    LogSleuth release automation script.

.DESCRIPTION
    Validates and bumps the semantic version, updates Cargo.toml and Cargo.lock,
    runs a release build and full test suite, commits the version bump, creates
    an annotated git tag, pushes to origin, and prunes older release tags/GitHub
    releases so only the new tag remains.

    Dry-run mode (-DryRun) describes every planned action without making any changes.
    Force mode (-Force) allows overwriting an existing tag and re-releasing the same
    or an older version number.

.PARAMETER Version
    Target semantic version string (x.y.z). Prompted interactively if omitted.

.PARAMETER Notes
    Release notes for the annotated tag and GitHub release. Prompted interactively
    if omitted.

.PARAMETER Force
    Allow overwriting an existing tag or releasing without a version increment.

.PARAMETER DryRun
    Describe all planned actions without modifying any files, the git repository,
    or remote hosting.

.EXAMPLE
    # Interactive
    .\update-application.ps1

    # Fully scripted
    .\update-application.ps1 -Version 1.2.0 -Notes "Fix critical parser bug"

    # Re-release an existing tag
    .\update-application.ps1 -Version 1.2.0 -Notes "Rebuilt release" -Force

    # Preview without committing anything
    .\update-application.ps1 -Version 1.2.0 -Notes "Preview" -DryRun
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $false)]
    [string]$Version,

    [Parameter(Mandatory = $false)]
    [string]$Notes,

    [switch]$Force,

    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

# ---------------------------------------------------------------------------
# Coloured output helpers
# ---------------------------------------------------------------------------

function Write-Info([string]$Message) {
    Write-Host "[INFO] $Message" -ForegroundColor Cyan
}

function Write-Success([string]$Message) {
    Write-Host "[OK]   $Message" -ForegroundColor Green
}

function Write-WarnLine([string]$Message) {
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Write-ErrorLine([string]$Message) {
    Write-Host "[ERR]  $Message" -ForegroundColor Red
}

# ---------------------------------------------------------------------------
# Git helpers
# ---------------------------------------------------------------------------

function Invoke-Git([string[]]$Arguments) {
    & git @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "git $($Arguments -join ' ') failed"
    }
}

function Test-IsGitRepository {
    & git rev-parse --is-inside-work-tree *> $null
    return ($LASTEXITCODE -eq 0)
}

function Get-RemoteHttpsUrl {
    $remote = (& git config --get remote.origin.url)
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($remote)) {
        return $null
    }

    $remote = $remote.Trim()
    if ($remote -match '^https://') {
        return $remote -replace '\.git$', ''
    }

    if ($remote -match '^git@github\.com:(?<slug>[^\s]+?)(\.git)?$') {
        return "https://github.com/$($Matches['slug'])"
    }

    return $null
}

# ---------------------------------------------------------------------------
# Path resolution
# ---------------------------------------------------------------------------

function Get-WorkspaceRoot {
    $scriptDir = Split-Path -Parent $PSCommandPath
    return (Resolve-Path $scriptDir).Path
}

# ---------------------------------------------------------------------------
# Version helpers (single-crate [package] Cargo.toml)
# ---------------------------------------------------------------------------

function Get-PackageVersion([string]$CargoTomlPath) {
    $content = [System.IO.File]::ReadAllText($CargoTomlPath)
    # Match the version field inside the [package] block specifically.
    # Uses a non-greedy multiline scan that stops at the next section header.
    $match = [regex]::Match(
        $content,
        '(?ms)^\[package\].*?^version\s*=\s*"(?<version>\d+\.\d+\.\d+)"'
    )
    if (-not $match.Success) {
        throw "Could not locate [package] version in $CargoTomlPath"
    }
    return $match.Groups['version'].Value
}

function Compare-SemVer([string]$Left, [string]$Right) {
    $leftParts  = $Left.Split('.')  | ForEach-Object { [int]$_ }
    $rightParts = $Right.Split('.') | ForEach-Object { [int]$_ }
    for ($i = 0; $i -lt 3; $i++) {
        if ($leftParts[$i] -gt $rightParts[$i]) { return 1  }
        if ($leftParts[$i] -lt $rightParts[$i]) { return -1 }
    }
    return 0
}

function Update-PackageVersion([string]$CargoTomlPath, [string]$OldVersion, [string]$NewVersion) {
    $content = [System.IO.File]::ReadAllText($CargoTomlPath)

    # Replace the exact version string in the [package] block.
    # The pattern avoids replacing versions that appear inside [dependencies].
    $pattern     = '(?m)^(version\s*=\s*")' + [regex]::Escape($OldVersion) + '("\s*$)'
    $replacement = '${1}' + $NewVersion + '${2}'
    $updated     = $content -replace $pattern, $replacement

    if ($updated -eq $content) {
        throw "Package version replacement did not change $CargoTomlPath"
    }

    # Normalise to exactly one trailing newline, preserving the original line-ending
    # style (CRLF or LF) to avoid noisy git diffs.
    $eol      = if ($updated -match "`r`n") { "`r`n" } else { "`n" }
    $updated  = $updated.TrimEnd() + $eol

    $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($CargoTomlPath, $updated, $utf8NoBom)
}



# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

$workspaceRoot = Get-WorkspaceRoot
Set-Location $workspaceRoot

Write-Host ""
Write-Host "  LogSleuth Release Automation" -ForegroundColor White
Write-Host "  =============================" -ForegroundColor DarkGray
Write-Host ""

if ($DryRun) {
    Write-WarnLine "Dry-run mode enabled. No files, commits, tags, pushes, or releases will be changed."
    Write-Host ""
}

$cargoToml  = Join-Path $workspaceRoot "Cargo.toml"

$currentVersion = Get-PackageVersion -CargoTomlPath $cargoToml

Write-Host "Current version: " -NoNewline -ForegroundColor White
Write-Host "$currentVersion" -ForegroundColor Yellow
Write-Host ""

# --- Collect version ---
if (-not $Version) {
    $Version = Read-Host "Enter new semantic version (x.y.z)"
}
if ([string]::IsNullOrWhiteSpace($Version) -or ($Version -notmatch '^\d+\.\d+\.\d+$')) {
    throw "Version must match semantic versioning format x.y.z"
}

if (-not $Force) {
    $cmp = Compare-SemVer -Left $Version -Right $currentVersion
    if ($cmp -le 0) {
        throw "New version ($Version) must be greater than current version ($currentVersion). Use -Force to override."
    }
}

# --- Collect release notes ---
if (-not $Notes) {
    Write-Host "Enter release notes (end with an empty line):" -ForegroundColor Cyan
    $noteLines = [System.Collections.Generic.List[string]]::new()
    while ($true) {
        $line = Read-Host
        if ([string]::IsNullOrWhiteSpace($line)) { break }
        $noteLines.Add($line)
    }
    $Notes = $noteLines -join [Environment]::NewLine
}
if ([string]::IsNullOrWhiteSpace($Notes)) {
    throw "Release notes are required and cannot be empty"
}

# --- Git state checks ---
$isGitRepo = Test-IsGitRepository
if (-not $isGitRepo -and -not $DryRun) {
    throw "This script must be run inside a git repository."
}
if (-not $isGitRepo -and $DryRun) {
    Write-WarnLine "Git repository not detected. Git-dependent steps are skipped in dry-run mode."
}

$newTag      = "v$Version"
$existingTag = $null
if ($isGitRepo) {
    $existingTag = (& git tag -l $newTag)
    if (($LASTEXITCODE -eq 0) -and (-not [string]::IsNullOrWhiteSpace($existingTag)) -and (-not $Force)) {
        throw "Tag $newTag already exists. Use -Force to overwrite."
    }
}

$status = $null
if ($isGitRepo) {
    $status = & git status --porcelain
    if ($LASTEXITCODE -ne 0) { throw "Failed to inspect git status" }
    if (-not [string]::IsNullOrWhiteSpace($status)) {
        Write-WarnLine "Working tree has uncommitted changes. They will be included only if staged by this script."
    }
}

# --- Snapshot originals for rollback ---
$originalCargoToml  = [System.IO.File]::ReadAllText($cargoToml)
$lockPath           = Join-Path $workspaceRoot "Cargo.lock"
$lockExistedBefore  = Test-Path $lockPath
$originalLock       = $null
if ($lockExistedBefore) {
    $originalLock = [System.IO.File]::ReadAllText($lockPath)
}
$changedFiles = [System.Collections.Generic.List[string]]::new()

try {
    # -----------------------------------------------------------------------
    # Dry-run: summarise planned actions and exit
    # -----------------------------------------------------------------------
    if ($DryRun) {
        Write-Host "Release Summary (Dry Run)" -ForegroundColor White
        Write-Host "-------------------------" -ForegroundColor DarkGray
        Write-Host "Current version : $currentVersion"
        Write-Host "New version     : $Version"
        Write-Host "Tag             : $newTag"
        Write-Host "Release notes   :" -ForegroundColor White
        Write-Host $Notes
        Write-Host ""

        Write-Info "Planned actions:"
        if ($Version -ne $currentVersion) {
            Write-Host "  - Update [package] version in Cargo.toml: $currentVersion -> $Version"
            Write-Host "  - Run: cargo update"
        } else {
            Write-Host "  - No version bump needed (same version re-release)"
        }
        Write-Host "  - Run: cargo build --release  (Windows/host binary; macOS + Linux built by CI on tag push)"
        Write-Host "  - Run: cargo fmt -- --check"
        Write-Host "  - Run: cargo clippy -- -D warnings"
        Write-Host "  - Run: cargo test"
        if ($isGitRepo) {
            if ($Version -ne $currentVersion) {
                Write-Host "  - Run: git add Cargo.toml Cargo.lock"
                Write-Host "  - Run: git commit -m `"chore: bump version to $Version`""
            }
            Write-Host "  - Run: git tag -a $newTag -m <notes>"
            Write-Host "  - Run: git push origin HEAD"
            Write-Host "  - Run: git push origin $newTag"
            Write-Host "  - Prune older v*.*.* tags/releases (keep only $newTag)"
        } else {
            Write-Host "  - SKIP git actions (no git repository detected)"
        }
        Write-Host ""
        Write-Success "Dry-run complete. No changes made."
        exit 0
    }

    # -----------------------------------------------------------------------
    # Step 1 — Update version strings
    # -----------------------------------------------------------------------
    if ($Version -ne $currentVersion) {
        Write-Info "Updating [package] version in Cargo.toml: $currentVersion -> $Version"
        Update-PackageVersion -CargoTomlPath $cargoToml -OldVersion $currentVersion -NewVersion $Version
        $changedFiles.Add("Cargo.toml") | Out-Null

        Write-Info "Refreshing Cargo.lock via cargo update"
        & cargo update
        if ($LASTEXITCODE -ne 0) { throw "cargo update failed" }

        if (Test-Path $lockPath) {
            $changedFiles.Add("Cargo.lock") | Out-Null
        }
    } else {
        Write-WarnLine "Version unchanged ($Version) -- skipping Cargo.toml and lockfile update."
    }

    # -----------------------------------------------------------------------
    # Show summary and diff, then ask for confirmation
    # -----------------------------------------------------------------------
    Write-Host ""
    Write-Host "Release Summary" -ForegroundColor White
    Write-Host "---------------" -ForegroundColor DarkGray
    Write-Host "Current version : $currentVersion"
    Write-Host "New version     : $Version"
    Write-Host "Tag             : $newTag"
    Write-Host "Files to stage  : $($changedFiles -join ', ')"
    Write-Host "Release notes   :" -ForegroundColor White
    Write-Host $Notes
    Write-Host ""

    if ($isGitRepo -and $changedFiles.Count -gt 0) {
        Write-Info "Diff summary"
        & git --no-pager diff -- Cargo.toml Cargo.lock
        if ($LASTEXITCODE -ne 0) { throw "Failed to produce diff summary" }
    }

    $confirm = Read-Host "Proceed with build / test / release steps? (y/N)"
    if ($confirm -notin @('y', 'Y', 'yes', 'YES')) {
        throw "Release cancelled by user"
    }

    # -----------------------------------------------------------------------
    # Step 2 — Pre-release build (Windows / host target)
    #
    # This validates that the codebase compiles and produces the Windows binary.
    # macOS and Linux release binaries are built by the CI release.yml workflow
    # when the tag is pushed in Step 7 below.
    # -----------------------------------------------------------------------
    Write-Info "Running release build (Windows / host target)"
    & cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed" }

    # -----------------------------------------------------------------------
    # Step 3 — Format check, Clippy, and full test suite
    #
    # Mirrors the exact checks enforced by ci.yml so that failures surface here
    # rather than after the tag has already been pushed to origin.
    # -----------------------------------------------------------------------
    Write-Info "Checking formatting (cargo fmt -- --check)"
    & cargo fmt -- --check
    if ($LASTEXITCODE -ne 0) {
        throw "cargo fmt check failed -- run 'cargo fmt' to fix formatting, then re-run this script"
    }

    Write-Info "Running Clippy lints (cargo clippy -- -D warnings)"
    & cargo clippy -- -D warnings
    if ($LASTEXITCODE -ne 0) {
        throw "cargo clippy failed -- fix all warnings before releasing"
    }

    Write-Info "Running full test suite"
    & cargo test
    if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }

    # -----------------------------------------------------------------------
    # Step 4 — Handle existing tag (Force scenario)
    # -----------------------------------------------------------------------
    if ($Force -and -not [string]::IsNullOrWhiteSpace($existingTag)) {
        Write-WarnLine "Removing existing local tag $newTag (-Force)"
        Invoke-Git @('tag', '-d', $newTag)

        $remoteTagExists = & git ls-remote --tags origin $newTag
        if ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace($remoteTagExists)) {
            Write-WarnLine "Removing existing remote tag $newTag (-Force)"
            Invoke-Git @('push', 'origin', '--delete', $newTag)
        }
    }

    # -----------------------------------------------------------------------
    # Step 5 — Commit version bump
    # -----------------------------------------------------------------------
    if ($changedFiles.Count -gt 0) {
        Write-Info "Staging changed files"
        Invoke-Git @('add', 'Cargo.toml')
        if (Test-Path $lockPath) {
            Invoke-Git @('add', 'Cargo.lock')
        }

        # Only commit if staged content actually differs from HEAD.
        # A previous run may have already committed this version bump
        # before failing at a later step (tag/push); the file-level
        # rollback restores disk contents but cannot undo a git commit.
        & git diff --cached --quiet
        if ($LASTEXITCODE -ne 0) {
            Write-Info "Creating version bump commit"
            Invoke-Git @('commit', '-m', "chore: bump version to $Version")
        } else {
            Write-WarnLine "No staged changes vs HEAD -- version bump already committed. Skipping commit."
        }
    } else {
        Write-WarnLine "No version files changed -- skipping stage and commit."
    }

    # -----------------------------------------------------------------------
    # Step 6 — Tag and push
    # -----------------------------------------------------------------------
    Write-Info "Creating annotated tag $newTag"
    Invoke-Git @('tag', '-a', $newTag, '-m', $Notes)

    Write-Info "Pushing commit and tag to origin"
    Invoke-Git @('push', 'origin', 'HEAD')
    Invoke-Git @('push', 'origin', $newTag)

    # -----------------------------------------------------------------------
    # Step 7 — Prune older release tags
    # -----------------------------------------------------------------------
    Write-Info "Cleaning up older release tags (keeping $newTag)"
    $allReleaseTags = (& git tag -l 'v*.*.*') | Where-Object { $_ -ne $newTag }
    foreach ($oldTag in $allReleaseTags) {
        if ([string]::IsNullOrWhiteSpace($oldTag)) { continue }

        Write-Info "  Removing local tag $oldTag"
        Invoke-Git @('tag', '-d', $oldTag)

        $remoteTagExists = & git ls-remote --tags origin $oldTag
        if ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace($remoteTagExists)) {
            Write-Info "  Removing remote tag $oldTag"
            Invoke-Git @('push', 'origin', '--delete', $oldTag)
        }

        $ghAvailable = Get-Command gh -ErrorAction SilentlyContinue
        if ($ghAvailable) {
            & gh release delete $oldTag --yes
            if ($LASTEXITCODE -ne 0) {
                Write-WarnLine "Could not delete GitHub release for $oldTag (continuing)"
            }
        }
    }

    # -----------------------------------------------------------------------
    # Done
    # -----------------------------------------------------------------------
    $repoUrl = Get-RemoteHttpsUrl
    Write-Host ""
    if ($repoUrl) {
        Write-Success "Release $newTag submitted."
        Write-Success "Windows binary built locally."
        Write-Success "CI will build all platform binaries and publish the GitHub Release."
        Write-Success "Monitor pipeline: $repoUrl/actions"
    } else {
        Write-Success "Release $newTag submitted."
        Write-Success "Windows binary built locally."
        Write-Success "CI will build all platform binaries and publish the GitHub Release."
        Write-Success "Monitor CI/CD in your remote repository Actions page."
    }
    Write-Host ""
}
catch {
    Write-Host ""
    Write-ErrorLine $_.Exception.Message

    if ($DryRun) {
        exit 1
    }

    Write-WarnLine "Rolling back version file changes"

    $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($cargoToml, $originalCargoToml, $utf8NoBom)

    if ($lockExistedBefore -and $null -ne $originalLock) {
        [System.IO.File]::WriteAllText($lockPath, $originalLock, $utf8NoBom)
    } elseif (-not $lockExistedBefore -and (Test-Path $lockPath)) {
        Remove-Item -Path $lockPath -Force
    }

    Write-WarnLine "Rollback completed"
    Write-Host ""
    exit 1
}
