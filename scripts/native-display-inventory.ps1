# Read native display geometry using temporary, hidden, non-activating probe windows.
# https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getdpiforwindow
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class EmberDisplayInventory {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    public sealed class Display { public int X, Y, Width, Height; public uint Dpi; }
    delegate bool MonitorCallback(IntPtr monitor, IntPtr dc, ref Rect area, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumDisplayMonitors(IntPtr dc, IntPtr clip, MonitorCallback callback, IntPtr data);
    [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr CreateWindowExW(uint extended, string cls, string title, uint style, int x, int y, int width, int height, IntPtr parent, IntPtr menu, IntPtr instance, IntPtr parameter);
    [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] static extern bool DestroyWindow(IntPtr window);
    public static Display[] Read() {
        var old = SetThreadDpiAwarenessContext(new IntPtr(-4));
        if(old == IntPtr.Zero) throw new InvalidOperationException("DPI awareness unavailable");
        try {
            var displays = new List<Display>();
            MonitorCallback callback = delegate(IntPtr monitor, IntPtr dc, ref Rect area, IntPtr data) {
                var window = CreateWindowExW(0x08000000, "STATIC", "", 0x80000000, area.Left+20, area.Top+20, 1, 1, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero);
                if(window == IntPtr.Zero) throw new InvalidOperationException("Display probe unavailable");
                try { displays.Add(new Display { X=area.Left, Y=area.Top, Width=area.Right-area.Left, Height=area.Bottom-area.Top, Dpi=GetDpiForWindow(window) }); }
                finally { DestroyWindow(window); }
                return true;
            };
            if(!EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, callback, IntPtr.Zero)) throw new InvalidOperationException("Display enumeration failed");
            return displays.ToArray();
        } finally { SetThreadDpiAwarenessContext(old); }
    }
}
"@
$displays = @([EmberDisplayInventory]::Read())
[pscustomobject]@{ displays=$displays; mixedDpi=(@($displays | Select-Object -ExpandProperty Dpi -Unique).Count -gt 1); valid=(@($displays | Where-Object Dpi -eq 0).Count -eq 0) } | ConvertTo-Json -Depth 4
