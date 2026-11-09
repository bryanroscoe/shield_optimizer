# Open each file with no sharing and hold it until the signal file is gone,
# so a copy of it fails with a sharing violation. Used by the retry scenario.
param(
  [Parameter(Mandatory)] [string] $Signal,
  [Parameter(Mandatory, ValueFromRemainingArguments)] [string[]] $Files
)
$ErrorActionPreference = 'Stop'
$handles = foreach ($f in $Files) {
  [System.IO.File]::Open($f, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::None)
}
Set-Content "$Signal.ready" 'locked'
try {
  $deadline = (Get-Date).AddMinutes(15)
  while ((Test-Path $Signal) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 300 }
} finally {
  $handles | ForEach-Object { $_.Dispose() }
  Remove-Item "$Signal.ready" -ErrorAction SilentlyContinue
}
