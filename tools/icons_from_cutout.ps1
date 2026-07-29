<#
  Build the icon set from a pre-cut transparent PNG.

  Trims to the artwork's alpha bounding box, then centres it in a square canvas
  with a little padding, so the portal fills each icon instead of floating with
  dead space around it. Every size renders straight from the source, so nothing
  is resampled twice.

  Usage:
      powershell -ExecutionPolicy Bypass -File tools\icons_from_cutout.ps1 `
          -Source "path\to\cutout.png"
  then run `python tools\make_icons.py` to pack the .ico.
#>

param(
    [Parameter(Mandatory = $true)][string]$Source,
    # Fraction of the canvas the artwork occupies. 1.0 runs it edge to edge on
    # its longer axis; drop it below 1 to leave breathing room.
    [double]$Fill = 1.0
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$out = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\src-tauri\icons'))
New-Item -ItemType Directory -Force -Path $out | Out-Null

$src = New-Object System.Drawing.Bitmap((Resolve-Path $Source).Path)
Write-Host "source: $($src.Width)x$($src.Height) $($src.PixelFormat)"

# --- find the alpha bounding box --------------------------------------------
# LockBits rather than GetPixel: a megapixel of per-pixel calls through the
# PowerShell binder is unreasonably slow.

$rect = New-Object System.Drawing.Rectangle(0, 0, $src.Width, $src.Height)
$data = $src.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly,
                      [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$count = [Math]::Abs($data.Stride) * $src.Height
$buf = New-Object byte[] $count
[System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $buf, 0, $count)
$stride = $data.Stride
$src.UnlockBits($data)

$minX = $src.Width; $minY = $src.Height; $maxX = -1; $maxY = -1
for ($y = 0; $y -lt $src.Height; $y++) {
    $row = $y * $stride
    for ($x = 0; $x -lt $src.Width; $x++) {
        # Ignore near-transparent fringe pixels so soft edges don't inflate the box.
        if ($buf[$row + $x * 4 + 3] -gt 12) {
            if ($x -lt $minX) { $minX = $x }
            if ($x -gt $maxX) { $maxX = $x }
            if ($y -lt $minY) { $minY = $y }
            if ($y -gt $maxY) { $maxY = $y }
        }
    }
}

if ($maxX -lt 0) { throw "Source is fully transparent - is it really a cutout?" }
$bw = $maxX - $minX + 1
$bh = $maxY - $minY + 1
Write-Host "content bounds: ${bw}x${bh} at $minX,$minY"

$srcRect = New-Object System.Drawing.Rectangle($minX, $minY, $bw, $bh)

function Save-Icon([int]$size, [string]$path) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size,
        [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality

    # Preserve aspect ratio; centre whatever is left over.
    $avail = $size * $Fill
    $ratio = [Math]::Min($avail / $bw, $avail / $bh)
    $dw = [int]($bw * $ratio)
    $dh = [int]($bh * $ratio)
    $dx = [int](($size - $dw) / 2)
    $dy = [int](($size - $dh) / 2)

    $dst = New-Object System.Drawing.Rectangle($dx, $dy, $dw, $dh)
    $g.DrawImage($src, $dst, $srcRect, [System.Drawing.GraphicsUnit]::Pixel)
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Host "  wrote $(Split-Path $path -Leaf) (${size}x${size})"
}

Save-Icon 16  (Join-Path $out '16x16.png')
Save-Icon 32  (Join-Path $out '32x32.png')
Save-Icon 48  (Join-Path $out '48x48.png')
Save-Icon 64  (Join-Path $out '64x64.png')
Save-Icon 128 (Join-Path $out '128x128.png')
Save-Icon 256 (Join-Path $out '128x128@2x.png')
Save-Icon 512 (Join-Path $out 'icon.png')

$src.Dispose()
Write-Host "`nNow run: python tools\make_icons.py"
