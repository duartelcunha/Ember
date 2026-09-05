param(
    [string]$CaptureHelper,
    [switch]$AllowSingleMonitor,
    [string]$ExpectedVersion = ((Get-Content (Join-Path (Split-Path $PSScriptRoot -Parent) 'package.json') -Raw | ConvertFrom-Json).version),
    [string]$OutputDirectory = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target/native-qualification')
)
# A local Windows smoke test, not the full native qualification matrix. It opens
# synthetic background windows, moves the pointer and cancels the picker without
# selecting a project. It never reads the clipboard or starts a provider request.
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'This harness requires Windows' }
if ($CaptureHelper -and !(Test-Path -LiteralPath $CaptureHelper -PathType Leaf)) { throw 'Screenshot helper does not exist' }
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$reportPath = Join-Path $OutputDirectory ('picker-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.json')
Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class EmberNativeProbe {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    public delegate bool EnumCallback(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumCallback callback, IntPtr parameter);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window, out Rect rectangle);
    public static Rect[] VisibleRects(uint process) {
        var result = new List<Rect>();
        EnumWindows((window, parameter) => { uint owner; GetWindowThreadProcessId(window, out owner);
            Rect rectangle; if(owner == process && IsWindowVisible(window) && GetWindowRect(window, out rectangle)) result.Add(rectangle);
            return true;
        }, IntPtr.Zero);
        return result.ToArray();
    }
}
"@
[EmberNativeProbe]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$emberProcess = @(Get-Process ember -ErrorAction Stop)
if ($emberProcess.Count -ne 1) { throw 'Expected one installed Ember instance' }
$observedVersion = (Get-Item -LiteralPath $emberProcess[0].Path).VersionInfo.ProductVersion
if ($observedVersion -ne $ExpectedVersion) { throw "Expected Ember $ExpectedVersion, found $observedVersion" }
if (@([EmberNativeProbe]::VisibleRects($emberProcess[0].Id) | Where-Object { ($_.Right - $_.Left) -gt 32 -and ($_.Bottom - $_.Top) -gt 32 }).Count -ne 0) {
    throw 'Close Ember settings and finish any active interaction before running this harness'
}
$configuration = Get-Content -LiteralPath (Join-Path $env:APPDATA 'com.deleg8lab.ember/config.json') -Raw | ConvertFrom-Json
if ($configuration.hotkey_picker -ne 'CmdOrCtrl+Shift+P') { throw 'Picker shortcut differs from the qualified harness shortcut' }
$screens = @([System.Windows.Forms.Screen]::AllScreens | Sort-Object Primary -Descending)
if ($screens.Count -lt 2 -and !$AllowSingleMonitor) { throw 'Two monitors are required. Use -AllowSingleMonitor only to qualify the single-monitor scenario.' }
if ($screens.Count -lt 1) { throw 'No display is available' }
$originalCursor = [System.Windows.Forms.Cursor]::Position
$forms = @()
$observations = @()
$opened = $false
function Pump([int]$milliseconds) {
    $until = [DateTime]::UtcNow.AddMilliseconds($milliseconds)
    while ([DateTime]::UtcNow -lt $until) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 20 }
}
function Observe([string]$name, $screen, [int]$x, [int]$y) {
    if ([EmberNativeProbe]::GetForegroundWindow() -ne $primary.Handle) { throw "Focus changed before $name" }
    if (![EmberNativeProbe]::SetCursorPos($x, $y)) { throw 'Cursor movement failed' }
    Pump 800
    $focusPreserved = [EmberNativeProbe]::GetForegroundWindow() -eq $primary.Handle
    $area = $screen.WorkingArea
    $rectangles = @([EmberNativeProbe]::VisibleRects($emberProcess[0].Id))
    $matching = @($rectangles | Where-Object { $_.Left -eq $area.Left -and $_.Top -eq $area.Top -and $_.Right -eq $area.Right -and $_.Bottom -eq $area.Bottom })
    $captureX = [Math]::Max($area.Left, [Math]::Min($x - 320, $area.Right - 640))
    $captureY = [Math]::Max($area.Top, [Math]::Min($y - 200, $area.Bottom - 400))
    $imagePath = $null
    if ($CaptureHelper) {
        $imagePath = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $CaptureHelper -Mode temp -Region "$captureX,$captureY,640,400"
        if ($LASTEXITCODE -ne 0) { throw 'Screenshot failed' }
    }
    $script:observations += [pscustomobject]@{ version=$observedVersion; scenario=$name; focusPreserved=$focusPreserved; monitorSurfaceMatches=($matching.Count -eq 1); screenshot=$imagePath; visibleWindows=$rectangles }
    if (!$focusPreserved -or $matching.Count -ne 1) { throw "Native picker invariant failed in $name" }
}
try {
    foreach ($screen in $screens) {
        $form = New-Object System.Windows.Forms.Form
        $form.Text = 'Ember native qualification fixture'
        $form.StartPosition = 'Manual'
        $form.FormBorderStyle = 'None'
        $form.Bounds = $screen.WorkingArea
        $form.BackColor = [Drawing.Color]::FromArgb(242,242,242)
        $label = New-Object System.Windows.Forms.Label
        $label.Text = 'Ember native picker test. Synthetic background; no clipboard or provider request.'
        $label.AutoSize = $true
        $label.Location = New-Object Drawing.Point(40,40)
        $form.Controls.Add($label)
        $form.Show()
        $forms += $form
    }
    $primary = $forms[0]
    $primary.Activate()
    Pump 1000
    if ([EmberNativeProbe]::GetForegroundWindow() -ne $primary.Handle) { throw 'Fixture did not obtain focus; no shortcut sent' }
    [System.Windows.Forms.SendKeys]::SendWait('^+p')
    $opened = $true
    Pump 1000
    $first = $screens[0]; $second = $screens[[Math]::Min(1, $screens.Count - 1)]
    $secondName = if ($screens.Count -gt 1) { "secondary" } else { "primary" }
    Observe 'primary-center' $first ($first.WorkingArea.Left + 800) ($first.WorkingArea.Top + 450)
    Observe 'primary-right-edge' $first ($first.WorkingArea.Right - 12) ($first.WorkingArea.Top + 450)
    Observe "$secondName-left-edge" $second ($second.WorkingArea.Left + 12) ($second.WorkingArea.Top + 450)
    Observe "$secondName-bottom-right" $second ($second.WorkingArea.Right - 12) ($second.WorkingArea.Bottom - 12)
    Observe 'return-primary' $first ($first.WorkingArea.Left + 800) ($first.WorkingArea.Top + 450)
    [System.Windows.Forms.SendKeys]::SendWait('{ESC}')
    $opened = $false
    Pump 800
    if (@([EmberNativeProbe]::VisibleRects($emberProcess[0].Id) | Where-Object { ($_.Right - $_.Left) -gt 32 -and ($_.Bottom - $_.Top) -gt 32 }).Count -ne 0) { throw 'Ember floating window remained visible after cancellation' }
    $after = Get-Content -LiteralPath (Join-Path $env:APPDATA 'com.deleg8lab.ember/config.json') -Raw | ConvertFrom-Json
    if ($after.active_project -ne $configuration.active_project -or $after.project_context -ne $configuration.project_context) { throw 'Cancellation changed project selection' }
    Write-Output "Native picker smoke passed: 5 positions, $([Math]::Min(2, $screens.Count)) monitor(s), focus preserved, Escape closed the surface."
} finally {
    if ($opened -and $primary -and [EmberNativeProbe]::GetForegroundWindow() -eq $primary.Handle) { [System.Windows.Forms.SendKeys]::SendWait('{ESC}'); Pump 400 }
    [System.Windows.Forms.Cursor]::Position = $originalCursor
    foreach ($form in $forms) { $form.Close(); $form.Dispose() }
    $observations | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $reportPath -Encoding UTF8
    $observations | ConvertTo-Json -Depth 5 | Write-Output
    Write-Output "Native report: $reportPath"
}
