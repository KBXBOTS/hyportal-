<#
  Derive a wordmark-free HyPortal icon from the full brand lockup.

  Takes the portal arch out of the source logo, scales it up to fill the frame,
  and redraws the bracketed border a little heavier. The result is square and
  reads at icon sizes, where the "HyPortal" text never could.

  Usage:
      powershell -ExecutionPolicy Bypass -File tools\make_mark.ps1 `
          -Source "path\to\logo.png" -Out "path\to\mark.png"
#>

param(
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$Size = 1024,
    # How much of the frame the portal fills, 0-1.
    [double]$PortalScale = 0.90,
    # Luminance at or above which pixels are left alone. Raise it to darken more
    # of the image, lower it to protect more of the stonework.
    [int]$BlackPoint = 90,
    # Curve steepness below the black point. Higher crushes harder.
    [double]$Falloff = 1.2,
    # Drop the background to full transparency and omit the border frame,
    # leaving just the portal on an empty canvas.
    [switch]$Transparent,
    # The edge-flood spreads through pixels dimmer than this. Raise it to reach
    # further into the glow; lower it if the fill leaks into the artwork.
    [int]$FloodMax = 70,
    # Blue excess (B - R) above which a pixel reads as the portal's bloom rather
    # than stonework, letting the fill travel through the glow. Stone is close to
    # neutral, so a modest value separates them cleanly.
    [int]$ChromaMin = 26,
    # Ceiling for bloom pixels, so the fill stops at the portal's bright core.
    [int]$BloomMax = 165,
    # Box-blur radius on the alpha channel, softening the cut edge. 0 disables.
    [int]$Feather = 1
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$src = [System.Drawing.Image]::FromFile((Resolve-Path $Source))
$W = $src.Width
Write-Host "source: ${W}x$($src.Height)"

# A square region of the source centred on the portal. It is scaled to fill the
# whole canvas, so every output pixel comes from the source - no compositing
# against a synthetic background, and therefore no visible seam.
#
# The lower edge stops at 0.57, just above the wordmark's top at ~0.578, so no
# letter tops creep into the frame.
$cropSize = $W * 0.43
$px = [int]($W * 0.5 - $cropSize / 2)
$py = [int]($W * 0.14)
$pw = [int]$cropSize
$ph = [int]$cropSize

$canvas = New-Object System.Drawing.Bitmap($Size, $Size)
$g = [System.Drawing.Graphics]::FromImage($canvas)
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality

# --- the portal, scaled to fill the whole canvas -----------------------------

$srcRect = New-Object System.Drawing.Rectangle($px, $py, $pw, $ph)

if ($Transparent) {
    # Leave a margin so the cut-out doesn't collide with the canvas edge.
    $inner = [int]($Size * $PortalScale)
    $off = [int](($Size - $inner) / 2)
    $dstRect = New-Object System.Drawing.Rectangle($off, $off, $inner, $inner)
} else {
    $dstRect = New-Object System.Drawing.Rectangle(0, 0, $Size, $Size)
}
$g.DrawImage($src, $dstRect, $srcRect, [System.Drawing.GraphicsUnit]::Pixel)
Write-Host "portal: ${pw}x${ph} from ($px,$py) -> $($dstRect.Width)x$($dstRect.Height)"

# --- crush the navy field to black ------------------------------------------
#
# The source sits the portal on dark blue with a soft bloom. Rolling everything
# below $BlackPoint toward zero on a curve removes that haze without touching
# the bright portal or the mid-tone stonework: a pixel at the threshold keeps
# ~100% of its value, one far below it keeps almost none.
#
# LockBits rather than GetPixel - a million per-pixel calls through the
# PowerShell binder would take minutes.

$rect = New-Object System.Drawing.Rectangle(0, 0, $Size, $Size)
$data = $canvas.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadWrite,
                         [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$count = [Math]::Abs($data.Stride) * $Size
$buf = New-Object byte[] $count
[System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $buf, 0, $count)

# Precompute the curve so the inner loop is table lookups, not Math::Pow calls.
$curve = New-Object double[] 256
for ($l = 0; $l -lt 256; $l++) {
    if ($l -ge $BlackPoint) { $curve[$l] = 1.0 }
    else { $curve[$l] = [Math]::Pow($l / $BlackPoint, $Falloff) }
}

if ($Transparent) {
    # Connectivity, not brightness - see BackgroundKey.cs for why a threshold
    # alone destroys the stonework. Skip the crush entirely: darkening the
    # subject first only makes it harder to tell apart from the backdrop.
    Add-Type -Path (Join-Path $PSScriptRoot 'BackgroundKey.cs')
    [BackgroundKey]::Apply($buf, $Size, $Size, $data.Stride,
                           $FloodMax, $ChromaMin, $BloomMax, $Feather)
    Write-Host "flood-keyed (lum < $FloodMax, or chroma > $ChromaMin and lum < $BloomMax)"
}
else {
    for ($i = 0; $i -lt $count; $i += 4) {
        # Format32bppArgb is little-endian: B, G, R, A.
        $b = $buf[$i]; $gr = $buf[$i + 1]; $r = $buf[$i + 2]
        $lum = [int](0.299 * $r + 0.587 * $gr + 0.114 * $b)
        if ($lum -lt $BlackPoint) {
            $f = $curve[$lum]
            $buf[$i] = [byte]($b * $f)
            $buf[$i + 1] = [byte]($gr * $f)
            $buf[$i + 2] = [byte]($r * $f)
        }
    }
    Write-Host "background crushed (black point $BlackPoint, falloff $Falloff)"
}

[System.Runtime.InteropServices.Marshal]::Copy($buf, 0, $data.Scan0, $count)
$canvas.UnlockBits($data)

# --- border: heavier than the original, with corner brackets ----------------
#
# Skipped for the transparent cut-out, which is meant to be just the portal.

if (-not $Transparent) {

# All border geometry scales with the canvas, so each size is drawn crisply
# rather than resampled from a larger render. A 1px floor keeps it visible at
# 16px instead of vanishing to nothing.
$inset = [Math]::Max(1, [int]($Size * 0.030))
$stroke = [Math]::Max(1, [int]($Size * 0.009))
$arm = [Math]::Max(2, [int]($Size * 0.14))   # length of each corner bracket arm
$blue = [System.Drawing.Color]::FromArgb(255, 47, 162, 255)

$glowPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(60, 47, 162, 255), ($stroke * 3))
$pen = New-Object System.Drawing.Pen($blue, $stroke)
$pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round

$lo = $inset
$hi = $Size - $inset
$half = [int]($arm * 0.55)

# Draw each segment twice: a wide translucent pass for the glow, then the
# crisp line on top.
function Seg([int]$x1, [int]$y1, [int]$x2, [int]$y2) {
    $script:g.DrawLine($script:glowPen, $x1, $y1, $x2, $y2)
    $script:g.DrawLine($script:pen, $x1, $y1, $x2, $y2)
}

# Straight runs between the corner brackets; the corners stay open, as in the
# source art.
Seg ($lo + $arm) $lo ($hi - $arm) $lo
Seg ($lo + $arm) $hi ($hi - $arm) $hi
Seg $lo ($lo + $arm) $lo ($hi - $arm)
Seg $hi ($lo + $arm) $hi ($hi - $arm)

# Corner brackets: two short arms meeting at each corner.
Seg $lo ($lo + $half) $lo $lo
Seg $lo $lo ($lo + $half) $lo
Seg ($hi - $half) $lo $hi $lo
Seg $hi $lo $hi ($lo + $half)
Seg $lo ($hi - $half) $lo $hi
Seg $lo $hi ($lo + $half) $hi
Seg ($hi - $half) $hi $hi $hi
Seg $hi ($hi - $half) $hi $hi

# Small diamond accents at the midpoint of each edge, as in the source.
$mid = [int]($Size / 2)
$d = [int]($Size * 0.014)
$fill = New-Object System.Drawing.SolidBrush($blue)

function Diamond([int]$cx, [int]$cy, [int]$r) {
    $pts = New-Object 'System.Drawing.Point[]' 4
    $pts[0] = New-Object System.Drawing.Point($cx, ($cy - $r))
    $pts[1] = New-Object System.Drawing.Point(($cx + $r), $cy)
    $pts[2] = New-Object System.Drawing.Point($cx, ($cy + $r))
    $pts[3] = New-Object System.Drawing.Point(($cx - $r), $cy)
    $script:g.FillPolygon($script:fill, $pts)
}

Diamond $mid $lo $d
Diamond $mid $hi $d
Diamond $lo $mid $d
Diamond $hi $mid $d

$fill.Dispose(); $pen.Dispose(); $glowPen.Dispose()

}  # end: -not $Transparent
$canvas.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $canvas.Dispose(); $src.Dispose()
Write-Host "wrote $Out (${Size}x${Size})"
