param([int]$ProcessId)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class RatingTestWindow {
    private delegate bool Callback(IntPtr w, IntPtr data);
    [DllImport("user32.dll")] private static extern bool EnumWindows(Callback callback, IntPtr data);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr w, out uint pid);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr w);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowTextW(IntPtr w, StringBuilder title, int size);
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr w, uint m, IntPtr a, IntPtr b);
    public static IntPtr[] Visible(int pid) {
        var result = new List<IntPtr>();
        EnumWindows((w, d) => {
            uint owner; GetWindowThreadProcessId(w, out owner);
            var title = new StringBuilder(256); GetWindowTextW(w, title, 256);
            if (owner == pid && IsWindowVisible(w) && title.ToString().StartsWith("Noxdeck", StringComparison.OrdinalIgnoreCase)) result.Add(w);
            return true;
        }, IntPtr.Zero);
        return result.ToArray();
    }
}
'@
$windows = [RatingTestWindow]::Visible($ProcessId)
if ($windows.Count -ne 1) { throw 'Expected one isolated app window' }
if (-not [RatingTestWindow]::PostMessageW($windows[0], 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'WM_CLOSE failed' }
for ($i=0; $i -lt 100; $i++) {
    if ([RatingTestWindow]::Visible($ProcessId).Count -eq 0) { exit 0 }
    Start-Sleep -Milliseconds 20
}
throw 'App window remained visible'
