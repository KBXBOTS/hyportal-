<#
  Resize the HyPortal brand logo into the PNG sizes Tauri bundles.

  Every size uses the complete logo, uncropped, so the icon always reads as the
  full brand mark.

  Usage:
      powershell -ExecutionPolicy Bypass -File tools\make_icons.ps1 -Source path\to\logo.png
  then run `python tools\make_icons.py` to pack the .ico.
#>

param(
    [Parameter(Mandatory = $true)]
    [string]$Source
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$out = Join-Path $PSScriptRoot '..\src-tauri\icons'
$out = [System.IO.Path]::GetFullPath($out)
New-Item -ItemType Directory -Force -Path $out | Out-Null

$img = [System.Drawing.Image]::FromFile((Resolve-Path $Source))
Write-Host "source: $($img.Width)x$($img.Height)"

# Fractions of the source width, framing the portal arch and its glow while
# stopping short of the wordmark, whose top edge sits at about 0.58.
$cropX = [int]($img.Width * 0.295)
$cropY = [int]($img.Width * 0.155)
$cropS = [int]($img.Width * 0.410)

function Save-Icon($image, [int]$size, [string]$path, $srcRect) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $dest = New-Object System.Drawing.Rectangle(0, 0, $size, $size)
    $g.DrawImage($image, $dest, $srcRect, [System.Drawing.GraphicsUnit]::Pixel)
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Host "  wrote $(Split-Path $path -Leaf) (${size}x${size})"
}

$full = New-Object System.Drawing.Rectangle(0, 0, $img.Width, $img.Height)

# The complete lockup at every size - never cropped.
Save-Icon $img 16  (Join-Path $out '16x16.png')      $full
Save-Icon $img 32  (Join-Path $out '32x32.png')      $full
Save-Icon $img 48  (Join-Path $out '48x48.png')      $full
Save-Icon $img 64  (Join-Path $out '64x64.png')      $full
Save-Icon $img 128 (Join-Path $out '128x128.png')    $full
Save-Icon $img 256 (Join-Path $out '128x128@2x.png') $full
Save-Icon $img 512 (Join-Path $out 'icon.png')       $full

$img.Dispose()
Write-Host "`nNow run: python tools\make_icons.py"
