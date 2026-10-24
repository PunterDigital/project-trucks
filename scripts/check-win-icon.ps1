# Fails unless every .exe given shows the app icon (src-tauri/icons/32x32.png, the same image as icon.ico's
# 32x32 frame), not Tauri's or NSIS's default icon. System.Drawing.Icon can't decode the PNG-compressed frames
# that `tauri icon` writes into icon.ico (it returns noise), so the reference is the PNG.
# Usage: powershell -ExecutionPolicy Bypass -File scripts/check-win-icon.ps1 <exe> [<exe> ...]
param([Parameter(Mandatory, ValueFromRemainingArguments)][string[]]$Exe)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$want = [System.Drawing.Bitmap]::FromFile((Join-Path $PSScriptRoot '..\src-tauri\icons\32x32.png'))
if ($want.Width -ne 32 -or $want.Height -ne 32) { throw '32x32.png is not 32x32' }
$failed = $false
foreach ($f in $Exe) {
  $got = [System.Drawing.Icon]::ExtractAssociatedIcon((Resolve-Path $f).Path).ToBitmap()
  $diff = 0
  for ($x = 0; $x -lt 32; $x++) {
    for ($y = 0; $y -lt 32; $y++) {
      $a = $want.GetPixel($x, $y); $b = $got.GetPixel($x, $y)
      if ([Math]::Abs($a.R - $b.R) + [Math]::Abs($a.G - $b.G) + [Math]::Abs($a.B - $b.B) + [Math]::Abs($a.A - $b.A) -gt 16) { $diff++ }
    }
  }
  Write-Output "${f}: $diff of 1024 pixels differ from src-tauri/icons/32x32.png"
  if ($diff -gt 20) { $failed = $true }
}
if ($failed) { throw 'An executable does not use the app icon' }
