param([int]$ProcessId, [string]$AppPath, [switch]$Resize)
$ErrorActionPreference = 'Stop'
$process = Get-CimInstance Win32_Process -Filter "ProcessId=$ProcessId"
if (-not $process -or $process.ExecutablePath -ne (Resolve-Path -LiteralPath $AppPath).Path) {
    throw 'Window check must target the isolated test application'
}
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class WindowStateCheck {
    private delegate bool Callback(IntPtr window, IntPtr data);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] private static extern bool EnumWindows(Callback callback, IntPtr data);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowTextW(IntPtr window, StringBuilder title, int size);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    public static IntPtr[] Find(int pid) {
        var result = new List<IntPtr>();
        EnumWindows((window, data) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            var title = new StringBuilder(256); GetWindowTextW(window, title, 256);
            if (owner == pid && title.ToString() == "Noxdeck") result.Add(window);
            return true;
        }, IntPtr.Zero);
        return result.ToArray();
    }
}
'@
$windows = [WindowStateCheck]::Find($ProcessId)
if ($windows.Count -eq 0) { 'null'; exit 0 }
if ($windows.Count -ne 1) { throw 'Expected one isolated app window' }
$window = $windows[0]
$rect = New-Object WindowStateCheck+Rect
$client = New-Object WindowStateCheck+Rect
if (-not [WindowStateCheck]::GetWindowRect($window, [ref]$rect) -or -not [WindowStateCheck]::GetClientRect($window, [ref]$client)) {
    throw 'Native size read failed'
}
if ($Resize) {
    $width = 800 + ($rect.Right - $rect.Left) - $client.Right
    $height = 600 + ($rect.Bottom - $rect.Top) - $client.Bottom
    if (-not [WindowStateCheck]::SetWindowPos($window, [IntPtr]::Zero, 100, 100, $width, $height, 0x0014)) {
        throw 'Native resize failed'
    }
    if (-not [WindowStateCheck]::GetWindowRect($window, [ref]$rect) -or -not [WindowStateCheck]::GetClientRect($window, [ref]$client)) {
        throw 'Native size read failed after resize'
    }
}
@{ normal = @{ x = $rect.Left; y = $rect.Top; width = $client.Right; height = $client.Bottom }; visible = [WindowStateCheck]::IsWindowVisible($window) } | ConvertTo-Json -Compress
