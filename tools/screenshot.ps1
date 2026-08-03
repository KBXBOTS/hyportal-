# Capture the HyPortal window to a PNG.
#
# PrintWindow, not CopyFromScreen: it asks the window to draw itself, so a
# window sitting on top of HyPortal does not end up in the shot.
#
# SetProcessDPIAware first, because PowerShell 5.1 is not DPI-aware by default
# and GetWindowRect would otherwise hand back virtualised coordinates, giving a
# picture cropped to roughly 1/scale of the real window.
param(
  [string]$Title = 'HyPortal',
  [string]$Out = "$env:TEMP\hyportal-shot.png"
)

Add-Type -AssemblyName System.Drawing

Add-Type @'
using System;
using System.Runtime.InteropServices;
public class Win {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
'@

[void][Win]::SetProcessDPIAware()

$proc = Get-Process | Where-Object { $_.MainWindowTitle -eq $Title } | Select-Object -First 1
if (-not $proc) { Write-Error "No window titled '$Title'"; exit 1 }

$h = $proc.MainWindowHandle
[void][Win]::SetForegroundWindow($h)
Start-Sleep -Milliseconds 400

$r = New-Object Win+RECT
[void][Win]::GetWindowRect($h, [ref]$r)
$w = $r.R - $r.L
$ht = $r.B - $r.T

$bmp = New-Object System.Drawing.Bitmap $w, $ht
$g = [System.Drawing.Graphics]::FromImage($bmp)
$dc = $g.GetHdc()
# 2 = PW_RENDERFULLCONTENT, required for the webview's composited surface.
[void][Win]::PrintWindow($h, $dc, 2)
$g.ReleaseHdc($dc)
$g.Dispose()

$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "$Out ${w}x${ht}"
