param(
    [Parameter(Mandatory = $true)] [string] $DllPath,
    [Parameter(Mandatory = $true)] [string] $DefPath,
    [Parameter(Mandatory = $true)] [string] $LibPath,
    [Parameter(Mandatory = $true)] [string] $Machine,
    [Parameter(Mandatory = $true)] [string] $Dumpbin,
    [Parameter(Mandatory = $true)] [string] $Lib
)

$ErrorActionPreference = "Stop"

if (!(Test-Path -LiteralPath $DllPath)) {
    throw "DLL not found: $DllPath"
}

$dllName = Split-Path -Leaf $DllPath
$dumpTool = Split-Path -Leaf $Dumpbin
if ($dumpTool -ieq "link.exe" -or $dumpTool -ieq "link") {
    $exports = & $Dumpbin /dump /nologo /exports $DllPath
} else {
    $exports = & $Dumpbin /nologo /exports $DllPath
}
if ($LASTEXITCODE -ne 0) {
    throw "$dumpTool failed for $DllPath"
}

$symbols = @()
foreach ($line in $exports) {
    if ($line -match '^\s*\d+\s+[0-9A-Fa-f]+\s+[0-9A-Fa-f]+\s+([^\s=]+)(?:\s*=.*)?\s*$') {
        $symbols += $Matches[1]
    }
}

if ($symbols.Count -eq 0) {
    throw "No exports found in $DllPath"
}

$def = @("LIBRARY $dllName", "EXPORTS") + ($symbols | Sort-Object -Unique | ForEach-Object { "  $_" })
$def | Set-Content -LiteralPath $DefPath -Encoding ASCII

& $Lib /nologo /def:$DefPath /machine:$Machine /out:$LibPath | Write-Host
if ($LASTEXITCODE -ne 0) {
    throw "lib.exe failed for $DllPath"
}
