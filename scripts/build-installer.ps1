#Requires -Version 5.1
[CmdletBinding()]
param([string]$MakeNsis = 'C:\Program Files (x86)\NSIS\makensis.exe')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$manifest = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw
$version = [regex]::Match($manifest, '(?ms)^\[package\].*?^version\s*=\s*"(\d+\.\d+\.\d+)"').Groups[1].Value
if (-not $version) { throw 'Cargo.toml has no package release version.' }
& $MakeNsis "/DPRODUCT_VERSION=$version" (Join-Path $root 'installer/windows/logsleuth.nsi')
if ($LASTEXITCODE -ne 0) { throw "NSIS failed with exit code $LASTEXITCODE" }
