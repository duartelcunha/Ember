param([Parameter(Mandatory=$true)][string]$CaptureHelper, [switch]$RequireMixedDpi)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path $PSScriptRoot -Parent
$executable = Join-Path $workspace 'target/debug/examples/floating_native.exe'
if (!(Test-Path -LiteralPath $executable)) { throw 'Build the native-qualification example first' }
$inventory = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'native-display-inventory.ps1') | ConvertFrom-Json
if (!$inventory.valid -or $inventory.displays.Count -lt 2) { throw 'Two valid displays are required' }
if ($RequireMixedDpi -and !$inventory.mixedDpi) { throw 'Release qualification requires two displays with different DPI scales; no cursor movement performed' }
Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class EmberFloatingProbe {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    public delegate bool EnumCallback(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumCallback callback, IntPtr data);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window, out Rect rectangle);
    public static uint Owner(IntPtr window) { uint process; GetWindowThreadProcessId(window, out process); return process; }
    public static Rect[] Rectangles(uint process) {
        var result = new List<Rect>();
        EnumWindows((window, data) => { uint owner; GetWindowThreadProcessId(window, out owner); Rect rectangle;
            if(owner == process && IsWindowVisible(window) && GetWindowRect(window, out rectangle)) result.Add(rectangle);
            return true;
        }, IntPtr.Zero);
        return result.ToArray();
    }
}
"@
$previousAwareness = [EmberFloatingProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
$original = New-Object EmberFloatingProbe+Point
[EmberFloatingProbe]::GetCursorPos([ref]$original) | Out-Null
$directory = Join-Path $workspace 'target/native-qualification'
New-Item -ItemType Directory -Force -Path $directory | Out-Null
$observations = @()
$started = Get-Date -Format 'yyyyMMdd-HHmmss'
$child = $null
try {
    foreach ($scene in @('orb','project','preview','hint')) {
        $before = [EmberFloatingProbe]::GetForegroundWindow()
        $log = Join-Path $directory "$started-$scene.jsonl"
        $errors = Join-Path $directory "$started-$scene.stderr"
        $child = Start-Process -FilePath $executable -ArgumentList $scene -WindowStyle Hidden -PassThru -RedirectStandardOutput $log -RedirectStandardError $errors
        Start-Sleep -Seconds 3
        if ($child.HasExited) { throw "Native fixture exited before $scene" }
        $native = Get-Content -LiteralPath $log | Select-Object -First 1 | ConvertFrom-Json
        foreach ($monitor in $native.monitors) {
            foreach ($edge in @($false,$true)) {
                $x = if ($edge) { $monitor.x + $monitor.width - 22 } else { $monitor.x + [int]($monitor.width / 2) }
                $y = if ($edge) { $monitor.y + $monitor.height - 22 } else { $monitor.y + [int]($monitor.height / 2) }
                if (![EmberFloatingProbe]::SetCursorPos($x,$y)) { throw 'Cursor movement failed' }
                Start-Sleep -Milliseconds 700
                $rectangles = @([EmberFloatingProbe]::Rectangles($child.Id))
                $matching = @($rectangles | Where-Object { $_.Left -eq $monitor.x -and $_.Top -eq $monitor.y -and ($_.Right - $_.Left) -eq $monitor.width -and ($_.Bottom - $_.Top) -eq $monitor.height })
                $foreground = [EmberFloatingProbe]::GetForegroundWindow()
                $focus = $foreground -eq $before
                $focusOwner = [EmberFloatingProbe]::Owner($foreground)
                $captureX = [Math]::Max($monitor.x,[Math]::Min($x-330,$monitor.x+$monitor.width-660))
                $captureY = [Math]::Max($monitor.y,[Math]::Min($y-220,$monitor.y+$monitor.height-440))
                $image = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $CaptureHelper -Mode temp -Region "$captureX,$captureY,660,440"
                if ($LASTEXITCODE -ne 0) { throw 'Screenshot failed' }
                $observations += [pscustomobject]@{scene=$scene; edge=$edge; x=$x; y=$y; scale=$monitor.scale; focusPreserved=$focus; focusOwnedByFixture=($focusOwner -eq $child.Id); foregroundOwner=$focusOwner; surfaceMatches=($matching.Count -eq 1); image=$image}
                if (!$focus -or $matching.Count -ne 1) { throw "Native surface invariant failed for $scene" }
            }
        }
        Stop-Process -Id $child.Id -ErrorAction SilentlyContinue
        $child = $null
    }
} finally {
    if ($child -and !$child.HasExited) { Stop-Process -Id $child.Id -ErrorAction SilentlyContinue }
    [EmberFloatingProbe]::SetCursorPos($original.X,$original.Y) | Out-Null
    [EmberFloatingProbe]::SetThreadDpiAwarenessContext($previousAwareness) | Out-Null
    $report = Join-Path $directory "$started-floating.json"
    [pscustomobject]@{artifactHash=(Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash; mixedDpi=$inventory.mixedDpi; observations=$observations} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $report -Encoding UTF8
    Write-Output "Report: $report"
}
Write-Output "Native scenarios: $($observations.Count). Mixed DPI: $($inventory.mixedDpi)"
