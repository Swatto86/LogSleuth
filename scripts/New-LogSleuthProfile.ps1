<#
.SYNOPSIS
    Generate a LogSleuth format profile (.toml) from a directory of log files.

.DESCRIPTION
    Scans a log directory, samples the first 50 lines of each unique filename-prefix
    group, infers the timestamp format, severity keywords, and line structure, then
    writes a ready-to-use .toml profile to the LogSleuth external profiles directory
    (or a path you specify).

    The generated profile is conservative: every inferred regex is commented where
    ambiguous so you can review and refine before using it.

    OUTPUT LOCATION (default):
        %APPDATA%\LogSleuth\profiles\<ProfileId>.toml

    To reload it in LogSleuth: Options > Reload Profiles  (or restart the app).

.PARAMETER LogDirectory
    Path to the directory containing the log files to sample.

.PARAMETER ProfileId
    Short machine-safe identifier for the profile, e.g. "my_app_log".
    Becomes the [profile] id field and the output filename.
    Defaults to the directory name, lowercased, spaces replaced with underscores.

.PARAMETER ProfileName
    Human-readable display name shown in LogSleuth.
    Defaults to "Auto: <ProfileId>".

.PARAMETER OutputPath
    Full path (including filename) to write the .toml file.
    Defaults to %APPDATA%\LogSleuth\profiles\<ProfileId>.toml.

.PARAMETER SampleLines
    Number of lines to read from the start of each representative file.
    Default: 50.

.PARAMETER Force
    Overwrite an existing .toml at the output path without prompting.

.EXAMPLE
    .\New-LogSleuthProfile.ps1 -LogDirectory "D:\Logs\MyApp" -ProfileId "myapp_log"

.EXAMPLE
    .\New-LogSleuthProfile.ps1 -LogDirectory "C:\inetpub\logs\LogFiles\W3SVC1" `
        -ProfileId "myapp_iis" -ProfileName "MyApp IIS Access" -Force

.NOTES
    Generated profiles use the LogSleuth TOML profile schema.
    Review and test with a real scan before relying on the auto-detected patterns.
    Requires PowerShell 5.1 or later.
#>

[CmdletBinding(SupportsShouldProcess)]
param(
    [Parameter(Mandatory = $true, HelpMessage = "Directory containing log files to sample")]
    [ValidateScript({ Test-Path $_ -PathType Container })]
    [string]$LogDirectory,

    [Parameter()]
    [string]$ProfileId = "",

    [Parameter()]
    [string]$ProfileName = "",

    [Parameter()]
    [string]$OutputPath = "",

    [Parameter()]
    [ValidateRange(10, 500)]
    [int]$SampleLines = 50,

    [Parameter()]
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------
$LOGSLEUTH_PROFILES_DIR = Join-Path $env:APPDATA "LogSleuth\profiles"
$MAX_REPRESENTATIVE_FILES = 5   # max files sampled per unique prefix group
$MIN_MATCH_RATIO = 0.6          # fraction of sample lines a pattern must match

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
function Write-Header {
    param([string]$Text)
    Write-Host "`n==> $Text" -ForegroundColor Cyan
}

function Write-Step {
    param([string]$Text)
    Write-Host "    $Text" -ForegroundColor DarkCyan
}

function Write-Ok {
    param([string]$Text)
    Write-Host "    OK  $Text" -ForegroundColor Green
}

function Write-Warn {
    param([string]$Text)
    Write-Host "    WRN $Text" -ForegroundColor Yellow
}

function ConvertTo-TomlString {
    param([string]$s)
    $s -replace '\\', '\\' -replace '"', '\"'
}

function ConvertTo-EscapedRegex {
    # Escape a literal string for use as a regex pattern
    param([string]$s)
    [System.Text.RegularExpressions.Regex]::Escape($s)
}

# ---------------------------------------------------------------------------
# Infer defaults
# ---------------------------------------------------------------------------
$resolvedDir = Resolve-Path $LogDirectory | Select-Object -ExpandProperty Path

if (-not $ProfileId) {
    $rawName = (Split-Path $resolvedDir -Leaf)
    $ProfileId = ($rawName -replace '\s+', '_' -replace '[^A-Za-z0-9_\-]', '').ToLower()
    if (-not $ProfileId) { $ProfileId = "custom_profile" }
}

if (-not $ProfileName) {
    $ProfileName = "Auto: $ProfileId"
}

if (-not $OutputPath) {
    if (-not (Test-Path $LOGSLEUTH_PROFILES_DIR)) {
        New-Item -ItemType Directory -Path $LOGSLEUTH_PROFILES_DIR -Force | Out-Null
    }
    $OutputPath = Join-Path $LOGSLEUTH_PROFILES_DIR "$ProfileId.toml"
}

# ---------------------------------------------------------------------------
# Guard: existing file
# ---------------------------------------------------------------------------
if ((Test-Path $OutputPath) -and -not $Force) {
    $answer = Read-Host "File '$OutputPath' already exists. Overwrite? [y/N]"
    if ($answer -notmatch '^[Yy]') {
        Write-Host "Aborted." -ForegroundColor Red
        exit 1
    }
}

Write-Header "LogSleuth Profile Generator"
Write-Step "Log directory : $resolvedDir"
Write-Step "Profile ID    : $ProfileId"
Write-Step "Profile name  : $ProfileName"
Write-Step "Output        : $OutputPath"
Write-Step "Sample lines  : $SampleLines"

# ---------------------------------------------------------------------------
# Step 1 — Collect log files
# ---------------------------------------------------------------------------
Write-Header "Step 1: Collecting log files"

$allFiles = Get-ChildItem -Path $resolvedDir -Recurse -File |
    Where-Object { $_.Extension -match '\.(log|txt|csv|out|trace|debug|log\d*)$' -or $_.Name -notmatch '\.' }

if (-not $allFiles) {
    Write-Warn "No recognised log files found in '$resolvedDir'."
    Write-Warn "Broadening search to all files..."
    $allFiles = Get-ChildItem -Path $resolvedDir -Recurse -File
}

Write-Step "Found $($allFiles.Count) candidate file(s)"

# Group by stem prefix (strip trailing digits / dates) to pick representatives
$groups = $allFiles | Group-Object {
    $stem = [System.IO.Path]::GetFileNameWithoutExtension($_.Name)
    # Strip trailing date/number suffixes: _20240101, -001, .1 etc.
    $stem -replace '[\-_\.]?\d{6,}', '' -replace '[\-_\.]\d+$', ''
}

$representative = foreach ($g in $groups) {
    $g.Group |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First $MAX_REPRESENTATIVE_FILES
}

Write-Step "Sampling $($representative.Count) representative file(s) from $($groups.Count) group(s)"

# ---------------------------------------------------------------------------
# Step 2 — Sample lines
# ---------------------------------------------------------------------------
Write-Header "Step 2: Sampling log content"

$sampledLines   = [System.Collections.Generic.List[string]]::new()
$fileExtensions = [System.Collections.Generic.HashSet[string]]::new()
$filePatterns   = [System.Collections.Generic.List[string]]::new()

foreach ($file in $representative) {
    Write-Step "Sampling: $($file.Name)"
    try {
        $lines = Get-Content -Path $file.FullName -TotalCount $SampleLines -Encoding UTF8 -ErrorAction Stop
        foreach ($line in $lines) {
            if ($line -and $line.Trim()) {
                $sampledLines.Add($line)
            }
        }
        $ext = $file.Extension.TrimStart('.')
        if ($ext) { [void]$fileExtensions.Add($ext.ToLower()) }

        # Collect stem pattern (replace numbers with * glob)
        $stem = [System.IO.Path]::GetFileNameWithoutExtension($file.Name)
        $stemPat = $stem -replace '\d{6,}', '*' -replace '\d+$', '*'
        $pat = if ($file.Extension) { "$stemPat$($file.Extension)" } else { $stemPat }
        if (-not $filePatterns.Contains($pat)) { $filePatterns.Add($pat) }
    }
    catch {
        Write-Warn "Could not read $($file.Name): $_"
    }
}

if ($sampledLines.Count -eq 0) {
    Write-Host "`nERROR: No readable lines found in any sampled file." -ForegroundColor Red
    exit 2
}

Write-Step "Collected $($sampledLines.Count) non-empty line(s)"

# ---------------------------------------------------------------------------
# Step 3 — Infer timestamp format
# ---------------------------------------------------------------------------
Write-Header "Step 3: Inferring timestamp format"

# Ordered list of (friendly-name, regex, strptime-format) candidates.
# We test each against up to $SampleLines lines and pick the best match ratio.
$timestampCandidates = @(
    @{ Name = "ISO 8601 / RFC 3339 (with timezone)";   Pattern = '^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})'; Format = "%Y-%m-%dT%H:%M:%S%.f%z" }
    @{ Name = "ISO 8601 (no timezone)";                Pattern = '^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?';                          Format = "%Y-%m-%d %H:%M:%S" }
    @{ Name = "ISO date, comma millis (log4j)";        Pattern = '^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2},\d+';                                   Format = "%Y-%m-%d %H:%M:%S,%3f" }
    @{ Name = "Veeam dot-date (DD.MM.YYYY HH:MM:SS)"; Pattern = '^\d{2}\.\d{2}\.\d{4} \d{2}:\d{2}:\d{2}';                                        Format = "%d.%m.%Y %H:%M:%S" }
    @{ Name = "Apache Combined (DD/Mon/YYYY:HH:MM:SS)"; Pattern = '\d{2}/\w{3}/\d{4}:\d{2}:\d{2}:\d{2}';                                          Format = "%d/%b/%Y:%H:%M:%S %z" }
    @{ Name = "US slash date (MM/DD/YYYY HH:MM)";     Pattern = '^\d{1,2}/\d{1,2}/\d{4} \d{2}:\d{2}';                                          Format = "%m/%d/%Y %H:%M" }
    @{ Name = "Windows DHCP (MM/DD/YY HH:MM:SS)";     Pattern = '^\d{1,2}/\d{1,2}/\d{2} \d{2}:\d{2}:\d{2}';                                    Format = "%m/%d/%y %H:%M:%S" }
    @{ Name = "BSD syslog (Mon DD HH:MM:SS)";         Pattern = '^[A-Z][a-z]{2}\s+\d{1,2} \d{2}:\d{2}:\d{2}';                                   Format = "%b %e %H:%M:%S" }
    @{ Name = "Month-name long (DD Mon YYYY HH:MM:SS)"; Pattern = '^\d{1,2} [A-Z][a-z]+ \d{4} \d{2}:\d{2}:\d{2}';                               Format = "%d %B %Y %H:%M:%S" }
)

$bestTsFormat   = "%Y-%m-%d %H:%M:%S"  # safe fallback
$bestTsName     = "ISO 8601 (fallback)"
$bestRatio      = 0.0
$bestCapture    = ""

foreach ($cand in $timestampCandidates) {
    $matched = ($sampledLines | Where-Object { $_ -match $cand.Pattern }).Count
    $ratio   = if ($sampledLines.Count -gt 0) { $matched / $sampledLines.Count } else { 0 }
    if ($ratio -gt $bestRatio) {
        $bestRatio   = $ratio
        $bestTsFormat = $cand.Format
        $bestTsName  = $cand.Name
        # Build a rough capture group from the anchored regex
        $bestCapture = "(?P<timestamp>" + $cand.Pattern.TrimStart('^') + ")"
    }
}

Write-Step "Best match : $bestTsName  (ratio=$([math]::Round($bestRatio,2)))"
Write-Step "strptime   : $bestTsFormat"

$tsConfident = ($bestRatio -ge $MIN_MATCH_RATIO)
if (-not $tsConfident) {
    Write-Warn "Low confidence ($([math]::Round($bestRatio,2)) < $MIN_MATCH_RATIO) -- timestamp field will be commented out for review."
}

# ---------------------------------------------------------------------------
# Step 4 — Infer content_match (characteristic line anchor)
# ---------------------------------------------------------------------------
Write-Header "Step 4: Inferring content signature"

# Try to find an anchor token present in >= 80% of lines.
# Strategy: pick 2–3 char n-grams that occur frequently in the first field.
$contentMatch = ""
$firstTokens = $sampledLines |
    ForEach-Object { ($_ -split '\s+')[0..2] -join ' ' } |
    Group-Object | Sort-Object Count -Descending | Select-Object -First 5

if ($firstTokens) {
    $candidate = $firstTokens[0].Name
    $ratio = $firstTokens[0].Count / $sampledLines.Count
    if ($ratio -ge 0.5 -and $candidate.Length -le 40) {
        $contentMatch = ConvertTo-EscapedRegex $candidate
        Write-Step "Content anchor: '$candidate'  (ratio=$([math]::Round($ratio,2)))"
    }
}

if (-not $contentMatch) {
    # Fallback: use the timestamp pattern start as the content anchor
    $contentMatch = ConvertTo-EscapedRegex ($timestampCandidates[0].Pattern.TrimStart('^'))
    Write-Warn "No reliable content anchor found -- using timestamp pattern as fallback"
}

# ---------------------------------------------------------------------------
# Step 5 — Infer severity keywords
# ---------------------------------------------------------------------------
Write-Header "Step 5: Searching for severity keywords"

$severityKeywords = @{
    Critical = @("\bCRITICAL\b", "\bFATAL\b", "\bCRIT\b")
    Error    = @("\bERROR\b", "\bERR\b", "\bFailed\b", "\bException\b")
    Warning  = @("\bWARN(?:ING)?\b", "\bWRN\b")
    Info     = @("\bINFO\b", "\bINFORMATION\b")
    Debug    = @("\bDEBUG\b", "\bDBG\b", "\bTRACE\b", "\bVERBOSE\b")
}

$foundSeverity = @{}
foreach ($kv in $severityKeywords.GetEnumerator()) {
    foreach ($pat in $kv.Value) {
        $matchCount = ($sampledLines | Where-Object { $_ -match $pat }).Count
        if ($matchCount -gt 0) {
            Write-Step "$($kv.Key): matched '$pat' in $matchCount line(s)"
            if (-not $foundSeverity.ContainsKey($kv.Key)) {
                $foundSeverity[$kv.Key] = $pat
            }
        }
    }
}

# ---------------------------------------------------------------------------
# Step 6 — Build file_patterns glob list
# ---------------------------------------------------------------------------
Write-Header "Step 6: Building file_patterns"

# Ensure common extensions are represented
foreach ($ext in @("log", "txt")) {
    if ($fileExtensions -contains $ext) {
        $pat = "*.$ext"
        if (-not $filePatterns.Contains($pat)) { $filePatterns.Add($pat) | Out-Null }
    }
}

# Add a catch-all if we have files without extensions
if ($fileExtensions -notcontains "log") {
    $filePatterns.Add("*.log") | Out-Null
}

Write-Step "Glob patterns: $($filePatterns -join ', ')"

# ---------------------------------------------------------------------------
# Step 7 — Compose the TOML profile
# ---------------------------------------------------------------------------
Write-Header "Step 7: Writing profile"

$lines      = [System.Collections.Generic.List[string]]::new()
$dateStamp  = (Get-Date -Format "yyyy-MM-dd")

$lines.Add("# LogSleuth format profile — generated by New-LogSleuthProfile.ps1")
$lines.Add("# Generated : $dateStamp")
$lines.Add("# Source    : $resolvedDir")
$lines.Add("# Review all patterns before shipping to production.")
$lines.Add("")
$lines.Add("[profile]")
$lines.Add("id          = `"$ProfileId`"")
$lines.Add("name        = `"$(ConvertTo-TomlString $ProfileName)`"")

# file_patterns array
$patsToml = ($filePatterns | ForEach-Object { "`"$_`"" }) -join ", "
$lines.Add("file_patterns = [$patsToml]")

# content_match
$lines.Add("")
$lines.Add("# content_match: regex that must appear in at least one of the first 20 lines.")
$lines.Add("# LogSleuth uses this (plus file_patterns) to auto-detect this format.")
if ($tsConfident) {
    $lines.Add("content_match = `"$(ConvertTo-TomlString $contentMatch)`"")
} else {
    $lines.Add("# content_match = `"$(ConvertTo-TomlString $contentMatch)`"  # LOW CONFIDENCE -- review required")
}

# line_pattern
$lines.Add("")
$lines.Add("# line_pattern: capture groups 'timestamp', 'severity' (optional), 'message'.")
if ($bestCapture) {
    $linePattern = $bestCapture + ".*"
    $lines.Add("# Tip: add (?P<severity>...) and (?P<message>.*) groups as needed.")
    if ($tsConfident) {
        $lines.Add("line_pattern = `"$(ConvertTo-TomlString $linePattern)`"")
    } else {
        $lines.Add("# line_pattern = `"$(ConvertTo-TomlString $linePattern)`"  # LOW CONFIDENCE -- review required")
    }
} else {
    $lines.Add("# line_pattern = `"(?P<timestamp>...)`"")
}

# timestamp_format
$lines.Add("")
$lines.Add("# timestamp_format: strptime string matching the captured timestamp.")
if ($tsConfident) {
    $lines.Add("timestamp_format = `"$(ConvertTo-TomlString $bestTsFormat)`"")
    $lines.Add("# Detected format: $bestTsName")
} else {
    $lines.Add("# timestamp_format = `"$(ConvertTo-TomlString $bestTsFormat)`"  # LOW CONFIDENCE -- review required")
    $lines.Add("# Closest match: $bestTsName  (ratio=$([math]::Round($bestRatio,2)))")
}

# severity_field (optional)
$lines.Add("")
$lines.Add("# Uncomment and complete severity_field if severity appears in a named capture group.")
$lines.Add("# severity_field = `"severity`"")

# severity_map (optional)
if ($foundSeverity.Count -gt 0) {
    $lines.Add("")
    $lines.Add("# Detected severity keywords (add to severity_map if needed):")
    $lines.Add("[profile.severity_map]")
    foreach ($kv in $foundSeverity.GetEnumerator() | Sort-Object Name) {
        $lines.Add("# $($kv.Key) = [`"...<pattern here>`"]")
    }
}

# log_locations (optional tooltip)
$lines.Add("")
$lines.Add("# log_locations: shown as a hover tooltip in the LogSleuth discovery panel.")
$lines.Add("# log_locations = `"Typical location: C:\\ProgramData\\MyApp\\Logs`"")

$tomlContent = $lines -join "`n"

# ---------------------------------------------------------------------------
# Write output
# ---------------------------------------------------------------------------
$outputDir = Split-Path $OutputPath
if ($outputDir -and -not (Test-Path $outputDir)) {
    New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
}

if ($PSCmdlet.ShouldProcess($OutputPath, "Write profile TOML")) {
    [System.IO.File]::WriteAllText($OutputPath, $tomlContent, [System.Text.Encoding]::UTF8)
    Write-Ok "Profile written to: $OutputPath"
}

Write-Host "`nDone. Next steps:" -ForegroundColor Green
Write-Host "  1. Open the file and review all patterns (especially low-confidence lines)" -ForegroundColor White
Write-Host "  2. In LogSleuth: Options > Reload Profiles" -ForegroundColor White
Write-Host "  3. Run a scan against your log directory and verify auto-detection fires" -ForegroundColor White
Write-Host ""
